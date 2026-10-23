//! Texture infrastructure: Places image assets as wgpu GPU textures.
//!
//! The engine hands this module decoded [`RawImage`] data with a source/content
//! identity. The cache owns GPU textures, mip chains, views and shared samplers.
//!
//! Colour images upload as `Rgba8UnormSrgb`, decoded by hardware before filtering.
//! CPU fitting and mips average linear light with coverage-weighted RGB, then
//! store sRGB bytes. Normal maps and masks remain raw `Rgba8Unorm` numeric data.
//!
//! Texture Filtering selects shared Low/Medium/High world samplers requesting
//! 4x/8x/16x anisotropy with linear magnification, minification and mip filtering.
//! Unsupported anisotropy clamps to 1x. The white fallback stays clamped nearest
//! with one mip; lightmaps use their own fixed clamped linear sampler.
//!
//! Identity includes source/content revision, semantic, quality class and
//! quality level. Repeated uses share an upload. Pack entries are level-scoped;
//! prior catalog/model entries have bounded retention at the next upload.
//! Active worlds keep independent `Arc`s. Lightmap atlases and render targets
//! are owned by their own modules.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::engine::materials::{RawImage, ResolvedTexture, TextureOrigin, decode_png};
use crate::engine::quality::{QualityLevel, TextureClass, fit_image};

/// Bytes per RGBA8 texel.
const RGBA_BYTES: u32 = 4;

/// The same four bytes as [`RGBA_BYTES`], for texel-chunk slice arithmetic.
const RGBA_CHANNELS: usize = 4;

/// Worker threads one texture batch may start.
///
/// One upload runs on a handful of sheets, so more workers than this cannot
/// help; the bound also keeps a batch from ever taking over a many-core
/// machine, exactly like the fixed bounds elsewhere in the engine.
const MAX_TEXTURE_WORKERS: usize = 8;

/// The shared untextured fallback, as committed artwork.
///
/// The same `assets/shared/textures/white_01.png` the reference renderer loaded
/// at startup (and embedded for an assets-less install): an opaque white
/// sheet sampled clamped and nearest, with no mip chain. It is a real
/// repository asset, not a generated fill; the decode below is the only path
/// that turns it into pixels.
const FALLBACK_WHITE_PNG: &[u8] = include_bytes!("../../../../assets/shared/textures/white_01.png");

/// The logical id of that sheet in the catalog.
const FALLBACK_TEXTURE_KEY: &str = "core:tex_white_01";

/// How one GPU texture's channels are meant to be interpreted.
///
/// [`Self::BaseColorDisplay`] covers authored colour; [`Self::DataLinear`]
/// covers numeric data (material normal maps, masks), whose cache key and
/// format the variant pins.
///
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureSemantic {
    /// Authored sRGB bytes; sampling returns linear RGB and raw alpha.
    BaseColorDisplay,
    /// Numeric data (a normal map, a mask, a lightmap). Also sampled raw.
    DataLinear,
}

impl TextureSemantic {
    /// The wgpu format this semantic uploads as.
    ///
    /// Colour decodes at sampling; numeric textures never undergo a transfer function.
    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::BaseColorDisplay => wgpu::TextureFormat::Rgba8UnormSrgb,
            Self::DataLinear => wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

/// How one GPU texture's UVs are addressed.
///
/// Tiling surface sheets repeat; a fitted single-use sheet (a prop model's own
/// texture, a fixture face) clamps, because its UVs never leave the image and a
/// repeat wrap would bleed one edge of the artwork into the opposite edge. A
/// sky sheet repeats horizontally (the equirectangular seam) and clamps at the
/// poles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureWrap {
    /// `REPEAT` addressing.
    Repeat,
    /// `CLAMP_TO_EDGE` addressing.
    Clamp,
    /// `REPEAT` in U (the sky's horizontal seam), `CLAMP_TO_EDGE` in V (its
    /// poles).
    RepeatClampV,
}

impl TextureWrap {
    /// The sampler policy for one filtering level at this wrap.
    #[must_use]
    pub const fn policy(self, filtering: TextureFiltering) -> SamplerPolicy {
        match self {
            Self::Repeat => filtering.sampler_policy(),
            Self::Clamp => filtering.clamp_sampler_policy(),
            Self::RepeatClampV => filtering.sky_sampler_policy(),
        }
    }
}

/// Model families share exact pixels independently of resource path. Semantic,
/// quality and wrapping remain separate `TextureKey` fields. A content key has
/// no mutable logical revision prefix to evict a different family's atlas.
pub(super) fn model_image_key(image: &RawImage) -> String {
    format!(
        "model-image-v1:{}:{}:{}",
        image.width,
        image.height,
        crate::engine::package::hash::sha256_hex(&image.rgba)
    )
}

fn revision_source(key: &str) -> Option<&str> {
    key.rsplit_once("#png-v1-")
        .or_else(|| key.rsplit_once("#rgba-v1-"))
        .map(|(source, _)| source)
}

/// The identity of one cache entry: the same source image can legitimately need
/// different GPU realizations.
///
/// The logical id is the Places session key (`core:tex_wallpaper_yellow_01`, or
/// `pack:<namespace>:<path>`); the remaining fields capture every input the GPU
/// realization depends on, so a clamped fixture sheet can never collide with the
/// same PNG used as a repeating surface, nor a future linear reading of a file
/// the colour path already uploaded.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TextureKey {
    /// Session-unique logical texture id.
    pub logical: String,
    /// Channel interpretation.
    pub semantic: TextureSemantic,
    /// The quality budget the fit applies to.
    pub class: TextureClass,
    /// The active quality level (the fit's edge budget).
    pub level: QualityLevel,
    /// How the UVs are addressed.
    pub wrap: TextureWrap,
}

impl TextureKey {
    /// A key for one resolved texture at one semantic and level, repeating.
    #[must_use]
    pub fn new(resolved: &ResolvedTexture, semantic: TextureSemantic, level: QualityLevel) -> Self {
        Self {
            logical: resolved.key.clone(),
            semantic,
            class: resolved.class,
            level,
            wrap: TextureWrap::Repeat,
        }
    }
}
/// The player's **Texture Filtering** setting for ordinary world textures.
///
/// All three levels are trilinear with anisotropic filtering; they differ only
/// in the requested anisotropy degree (Low 4x, Medium 8x, High 16x). The
/// setting is global, so the world shares one set of presets rather than one
/// sampler per texture, and switching is a bind-group handle swap at bind time
/// — no pixel data is re-uploaded.
///
/// Mipmaps are never disabled: every level filters linear/linear/linear, and
/// High keeps a source sheet at its native resolution (up to
/// [`crate::engine::assets::MAX_TEXTURE_DIMENSION`]); mip selection handles
/// minification. An adapter without anisotropic filtering keeps the same
/// linear levels with a 1x clamp
/// ([`SamplerPolicy::effective_descriptor`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureFiltering {
    /// 4x requested anisotropy, linear mag/min/mip.
    Low,
    /// 8x requested anisotropy, linear mag/min/mip.
    Medium,
    /// The default: 16x requested anisotropy, linear mag/min/mip.
    #[default]
    High,
}

impl TextureFiltering {
    /// Every level, in player-facing order.
    pub const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    /// The default level.
    pub const DEFAULT: Self = Self::High;

    /// Parses the player-facing setting.
    ///
    /// Surrounding whitespace is trimmed and the comparison is ASCII
    /// case-insensitive: `"low"`, `"medium"` and `"high"` are the levels. An
    /// empty or unknown value keeps the default (High).
    #[must_use]
    pub fn parse(mode: &str) -> Self {
        let trimmed_mode = mode.trim();
        if trimmed_mode.eq_ignore_ascii_case("low") {
            Self::Low
        } else if trimmed_mode.eq_ignore_ascii_case("medium") {
            Self::Medium
        } else {
            Self::High
        }
    }

    /// Stable name for diagnostics and persistence.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// The anisotropy degree this level requests.
    #[must_use]
    pub const fn anisotropy(self) -> u16 {
        match self {
            Self::Low => 4,
            Self::Medium => 8,
            Self::High => 16,
        }
    }

    /// The sampler policy this level selects for repeating world textures.
    #[must_use]
    pub const fn sampler_policy(self) -> SamplerPolicy {
        match self {
            Self::Low => SamplerPolicy::RepeatLow,
            Self::Medium => SamplerPolicy::RepeatMedium,
            Self::High => SamplerPolicy::RepeatHigh,
        }
    }

    /// The sampler policy this level selects for a clamped single-use sheet
    /// (a prop model's own sheet, a fixture face, an emissive mask).
    #[must_use]
    pub const fn clamp_sampler_policy(self) -> SamplerPolicy {
        match self {
            Self::Low => SamplerPolicy::ClampLow,
            Self::Medium => SamplerPolicy::ClampMedium,
            Self::High => SamplerPolicy::ClampHigh,
        }
    }

    /// The sampler policy this level selects for a sky sheet: the same world
    /// presets with U repeating and V clamped at the equirectangular poles.
    #[must_use]
    pub const fn sky_sampler_policy(self) -> SamplerPolicy {
        match self {
            Self::Low => SamplerPolicy::SkyLow,
            Self::Medium => SamplerPolicy::SkyMedium,
            Self::High => SamplerPolicy::SkyHigh,
        }
    }
}

/// One shared sampler configuration.
///
/// The six `Low`/`Medium`/`High` variants are the player's world presets: all
/// three filter linear in mag, min and mip and differ only in the requested
/// anisotropy. The remaining three policies never follow the player setting:
/// the retained point-sample pair and the clamped linear policy shared by the
/// lightmap atlas and the reflection targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SamplerPolicy {
    /// A repeating world sheet at the Low preset (4x requested anisotropy).
    RepeatLow,
    /// A repeating world sheet at the Medium preset (8x requested anisotropy).
    RepeatMedium,
    /// A repeating world sheet at the High preset (16x requested anisotropy).
    RepeatHigh,
    /// A clamped world sheet at the Low preset (4x requested anisotropy).
    ClampLow,
    /// A clamped world sheet at the Medium preset (8x requested anisotropy).
    ClampMedium,
    /// A clamped world sheet at the High preset (16x requested anisotropy).
    ClampHigh,
    /// A sky sheet at the Low preset: U repeats (the horizontal seam), V
    /// clamps at the poles (4x requested anisotropy).
    SkyLow,
    /// A sky sheet at the Medium preset (8x requested anisotropy).
    SkyMedium,
    /// A sky sheet at the High preset (16x requested anisotropy).
    SkyHigh,
    /// The retained point-sampled repeating policy; no world preset selects it.
    RepeatNearest,
    /// The fallback sheet and the UI's clamped point-sampled policy.
    ClampNearest,
    /// The lightmap atlas and reflection targets' clamped linear policy
    /// (anisotropy 1; deliberately independent of the player's world preset).
    ClampLinear,
}

impl SamplerPolicy {
    /// Stable label, used for the GPU object and the tests.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RepeatLow => "places-wgpu-sampler-repeat-low",
            Self::RepeatMedium => "places-wgpu-sampler-repeat-medium",
            Self::RepeatHigh => "places-wgpu-sampler-repeat-high",
            Self::ClampLow => "places-wgpu-sampler-clamp-low",
            Self::ClampMedium => "places-wgpu-sampler-clamp-medium",
            Self::ClampHigh => "places-wgpu-sampler-clamp-high",
            Self::SkyLow => "places-wgpu-sampler-sky-low",
            Self::SkyMedium => "places-wgpu-sampler-sky-medium",
            Self::SkyHigh => "places-wgpu-sampler-sky-high",
            Self::RepeatNearest => "places-wgpu-sampler-repeat-nearest",
            Self::ClampNearest => "places-wgpu-sampler-clamp-nearest",
            Self::ClampLinear => "places-wgpu-sampler-clamp-linear",
        }
    }

    /// The anisotropy degree this policy requests: 4/8/16 for the world
    /// presets, 1 for every specialized policy.
    #[must_use]
    pub const fn requested_anisotropy(self) -> u16 {
        match self {
            Self::RepeatLow | Self::ClampLow | Self::SkyLow => 4,
            Self::RepeatMedium | Self::ClampMedium | Self::SkyMedium => 8,
            Self::RepeatHigh | Self::ClampHigh | Self::SkyHigh => 16,
            Self::RepeatNearest | Self::ClampNearest | Self::ClampLinear => 1,
        }
    }

    /// The full descriptor, every field explicit: no wgpu default is relied on.
    ///
    /// Anisotropy above 1 requires all three filters to be linear, which the
    /// world presets satisfy by construction; the point-sampled and clamped
    /// linear policies request 1.
    #[must_use]
    pub const fn descriptor(self) -> wgpu::SamplerDescriptor<'static> {
        let (address_u, address_v, filter, mip) = match self {
            Self::RepeatLow | Self::RepeatMedium | Self::RepeatHigh => (
                wgpu::AddressMode::Repeat,
                wgpu::AddressMode::Repeat,
                wgpu::FilterMode::Linear,
                wgpu::MipmapFilterMode::Linear,
            ),
            Self::SkyLow | Self::SkyMedium | Self::SkyHigh => (
                wgpu::AddressMode::Repeat,
                wgpu::AddressMode::ClampToEdge,
                wgpu::FilterMode::Linear,
                wgpu::MipmapFilterMode::Linear,
            ),
            Self::ClampLow | Self::ClampMedium | Self::ClampHigh | Self::ClampLinear => (
                wgpu::AddressMode::ClampToEdge,
                wgpu::AddressMode::ClampToEdge,
                wgpu::FilterMode::Linear,
                wgpu::MipmapFilterMode::Linear,
            ),
            Self::RepeatNearest => (
                wgpu::AddressMode::Repeat,
                wgpu::AddressMode::Repeat,
                wgpu::FilterMode::Nearest,
                wgpu::MipmapFilterMode::Nearest,
            ),
            Self::ClampNearest => (
                wgpu::AddressMode::ClampToEdge,
                wgpu::AddressMode::ClampToEdge,
                wgpu::FilterMode::Nearest,
                wgpu::MipmapFilterMode::Nearest,
            ),
        };
        wgpu::SamplerDescriptor {
            label: Some(self.label()),
            address_mode_u: address_u,
            address_mode_v: address_v,
            address_mode_w: address_v,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: mip,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: self.requested_anisotropy(),
            border_color: None,
        }
    }

    /// The descriptor actually handed to the device.
    ///
    /// A world preset on an adapter without
    /// `DownlevelFlags::ANISOTROPIC_FILTERING` keeps its
    /// linear/linear/linear levels and clamps the anisotropy request to 1 —
    /// exactly what wgpu-core would silently do — so the fallback is visible in
    /// the descriptor rather than hidden in the backend. Specialized policies
    /// already request 1 and are unchanged either way.
    #[must_use]
    pub const fn effective_descriptor(
        self,
        anisotropy_supported: bool,
    ) -> wgpu::SamplerDescriptor<'static> {
        let mut descriptor = self.descriptor();
        if !anisotropy_supported {
            descriptor.anisotropy_clamp = 1;
        }
        descriptor
    }
}

/// The policy one uploaded sheet binds with one filtering level.
///
/// The shared fallback is deliberately sampled clamped-nearest in every slot
/// (it is a flat fill, so no filtering mode can change a texel); every other
/// sheet follows its own wrap contract and the selected world preset.
#[must_use]
pub const fn sheet_policy(
    wrap: TextureWrap,
    filtering: TextureFiltering,
    fallback: bool,
) -> SamplerPolicy {
    if fallback {
        SamplerPolicy::ClampNearest
    } else {
        wrap.policy(filtering)
    }
}

/// Plain-data facts about one uploaded texture, for diagnostics and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureMeta {
    /// Level-0 width in texels.
    pub width: u32,
    /// Level-0 height in texels.
    pub height: u32,
    /// Mip levels allocated and initialized.
    pub mip_levels: u32,
    /// The wgpu format the image uploaded as.
    pub format: wgpu::TextureFormat,
    /// The channel interpretation.
    pub semantic: TextureSemantic,
    /// The quality class whose budget fitted the image.
    pub class: TextureClass,
    /// The level the image was fitted for.
    pub level: QualityLevel,
    /// Where the texture came from; `Pack` is the level-scoped lifetime.
    pub origin: TextureOrigin,
    /// True for the shared fallback sheet.
    pub fallback: bool,
    /// Texel storage the texture occupies, all mip levels included.
    pub resident_bytes: u64,
}

impl TextureMeta {
    /// The longest level-0 edge, in texels.
    #[must_use]
    pub const fn max_edge(&self) -> u32 {
        if self.width > self.height {
            self.width
        } else {
            self.height
        }
    }
}

/// One uploaded texture and the bind groups a draw binds.
///
/// The texture itself is kept alongside the view and the bind groups: dropping
/// the struct releases the GPU allocation once no draw references it.
pub struct GpuTexture {
    /// Kept for ownership; the bind groups reference it.
    _texture: wgpu::Texture,
    /// Kept for ownership; the bind groups were built from it.
    view: wgpu::TextureView,
    /// Texture + the Low world sampler policy.
    low_bind_group: wgpu::BindGroup,
    /// Texture + the Medium world sampler policy.
    medium_bind_group: wgpu::BindGroup,
    /// Texture + the High world sampler policy.
    high_bind_group: wgpu::BindGroup,
    meta: TextureMeta,
}

impl GpuTexture {
    /// Uploads one prepared CPU mip chain as a GPU texture.
    ///
    /// `PreparedTexture::prepare` owns every CPU-side decision (the fit and the
    /// chain), so it may run on a worker thread; this function touches only the
    /// device and queue and stays on the caller's thread. Every allocated level
    /// is written before the function returns, so no level can be sampled
    /// uninitialized.
    fn upload_prepared(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        samplers: &Samplers,
        upload: &TextureUpload<'_>,
    ) -> Self {
        let prepared = upload.prepared;
        let format = upload.key.semantic.format();
        let mip_levels = prepared.mip_levels.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-texture"),
            size: wgpu::Extent3d {
                width: prepared.width.max(1),
                height: prepared.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        for (level, image) in prepared.levels.iter().enumerate() {
            write_mip(
                queue,
                &texture,
                u32::try_from(level).unwrap_or(u32::MAX),
                image,
            );
        }

        // The fallback sheet binds the reference's clamped nearest sampler in
        // every slot: it is a solid fill, so no filtering level can disagree
        // about a texel. Every other texture follows its own wrap contract and
        // the three world presets, so switching levels is a bind-group handle
        // swap with no re-upload.
        let bind_group_for = |filtering: TextureFiltering| {
            create_bind_group(
                device,
                layout,
                &view,
                samplers.get(sheet_policy(upload.key.wrap, filtering, upload.fallback)),
            )
        };
        let low_bind_group = bind_group_for(TextureFiltering::Low);
        let medium_bind_group = bind_group_for(TextureFiltering::Medium);
        let high_bind_group = bind_group_for(TextureFiltering::High);

        Self {
            _texture: texture,
            view,
            low_bind_group,
            medium_bind_group,
            high_bind_group,
            meta: TextureMeta {
                width: prepared.width,
                height: prepared.height,
                mip_levels,
                format,
                semantic: upload.key.semantic,
                class: upload.key.class,
                level: upload.key.level,
                origin: upload.origin,
                fallback: upload.fallback,
                resident_bytes: prepared.resident_bytes,
            },
        }
    }

    /// The bind group for one filtering level.
    ///
    /// The fallback sheet answers the same clamped nearest policy for all
    /// three.
    #[must_use]
    pub const fn bind_group(&self, filtering: TextureFiltering) -> &wgpu::BindGroup {
        match filtering {
            TextureFiltering::Low => &self.low_bind_group,
            TextureFiltering::Medium => &self.medium_bind_group,
            TextureFiltering::High => &self.high_bind_group,
        }
    }

    /// The texture view, for a caller that builds its own bind group against a
    /// different layout (a material's normal-map binding).
    ///
    /// Kept narrow on purpose: the texture and view stay private so the cache
    /// remains the only creator of GPU textures.
    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Plain-data facts about the upload.
    #[must_use]
    pub const fn meta(&self) -> &TextureMeta {
        &self.meta
    }

    /// True for the shared fallback sheet.
    #[must_use]
    pub const fn is_fallback(&self) -> bool {
        self.meta.fallback
    }
}

/// The explicit inputs of one [`GpuTexture`] upload.
struct TextureUpload<'a> {
    key: &'a TextureKey,
    prepared: &'a PreparedTexture,
    origin: TextureOrigin,
    fallback: bool,
}

/// One texture's CPU-side upload data.
///
/// [`Self::prepare`] owns everything the upload needs before the device is
/// touched — the quality fit and the full box-filtered mip chain — and the
/// result is plain owned data, so a batch can prepare several sheets on worker
/// threads without any `Arc` or GPU handle crossing a thread.
struct PreparedTexture {
    /// Level-0 width in texels, after the fit.
    width: u32,
    /// Level-0 height in texels, after the fit.
    height: u32,
    /// Mip levels in [`Self::levels`].
    mip_levels: u32,
    /// Level 0 first, one owned image per allocated level.
    levels: Vec<RawImage>,
    /// Texel storage every level occupies, matching [`TextureMeta`].
    resident_bytes: u64,
}

impl PreparedTexture {
    /// Fits `image` for the key's class and level and generates the full CPU
    /// mip chain.
    ///
    /// Pure CPU work with no shared state, so it is safe to run on a worker
    /// thread; the result is byte-identical whether it runs there or serially.
    fn prepare(
        image: &RawImage,
        level: QualityLevel,
        class: TextureClass,
        semantic: TextureSemantic,
    ) -> Self {
        let fit = fit_image(image, level, class);
        let fitted = if semantic == TextureSemantic::BaseColorDisplay
            && (fit.width != image.width || fit.height != image.height)
        {
            std::borrow::Cow::Owned(crate::engine::materials::color::resize_color(
                image, fit.width, fit.height,
            ))
        } else {
            fit
        };
        let width = fitted.width;
        let height = fitted.height;
        let mip_levels = mip_level_count(width, height);
        let mut levels: Vec<RawImage> =
            Vec::with_capacity(usize::try_from(mip_levels).unwrap_or(0));
        let mut resident_bytes = level_bytes(width, height);
        levels.push(fitted.into_owned());
        for _ in 1..mip_levels {
            let Some(parent) = levels.last() else {
                break;
            };
            let half = if semantic == TextureSemantic::BaseColorDisplay {
                crate::engine::materials::color::resize_color(
                    parent,
                    (parent.width / 2).max(1),
                    (parent.height / 2).max(1),
                )
            } else {
                halve_image(parent)
            };
            resident_bytes = resident_bytes.saturating_add(level_bytes(half.width, half.height));
            levels.push(half);
        }
        Self {
            width,
            height,
            mip_levels,
            levels,
            resident_bytes,
        }
    }

    /// One level exactly as given: the shared fallback sheet's contract (the
    /// reference never mip-chains it).
    fn single(image: RawImage) -> Self {
        let resident_bytes = level_bytes(image.width, image.height);
        Self {
            width: image.width,
            height: image.height,
            mip_levels: 1,
            levels: vec![image],
            resident_bytes,
        }
    }
}

/// The twelve shared samplers the world and every later pass use.
///
/// The six world presets are created with
/// [`SamplerPolicy::effective_descriptor`], so an adapter without anisotropic
/// filtering gets the same linear levels at a 1x clamp.
#[derive(Clone)]
struct Samplers {
    repeat_low: wgpu::Sampler,
    repeat_medium: wgpu::Sampler,
    repeat_high: wgpu::Sampler,
    clamp_low: wgpu::Sampler,
    clamp_medium: wgpu::Sampler,
    clamp_high: wgpu::Sampler,
    sky_low: wgpu::Sampler,
    sky_medium: wgpu::Sampler,
    sky_high: wgpu::Sampler,
    repeat_nearest: wgpu::Sampler,
    clamp_nearest: wgpu::Sampler,
    clamp_linear: wgpu::Sampler,
}

impl Samplers {
    /// Creates the shared samplers once per device.
    fn new(device: &wgpu::Device, anisotropy_supported: bool) -> Self {
        let create = |policy: SamplerPolicy| {
            device.create_sampler(&policy.effective_descriptor(anisotropy_supported))
        };
        Self {
            repeat_low: create(SamplerPolicy::RepeatLow),
            repeat_medium: create(SamplerPolicy::RepeatMedium),
            repeat_high: create(SamplerPolicy::RepeatHigh),
            clamp_low: create(SamplerPolicy::ClampLow),
            clamp_medium: create(SamplerPolicy::ClampMedium),
            clamp_high: create(SamplerPolicy::ClampHigh),
            sky_low: create(SamplerPolicy::SkyLow),
            sky_medium: create(SamplerPolicy::SkyMedium),
            sky_high: create(SamplerPolicy::SkyHigh),
            repeat_nearest: create(SamplerPolicy::RepeatNearest),
            clamp_nearest: create(SamplerPolicy::ClampNearest),
            clamp_linear: create(SamplerPolicy::ClampLinear),
        }
    }

    /// The sampler for a policy.
    const fn get(&self, policy: SamplerPolicy) -> &wgpu::Sampler {
        match policy {
            SamplerPolicy::RepeatLow => &self.repeat_low,
            SamplerPolicy::RepeatMedium => &self.repeat_medium,
            SamplerPolicy::RepeatHigh => &self.repeat_high,
            SamplerPolicy::ClampLow => &self.clamp_low,
            SamplerPolicy::ClampMedium => &self.clamp_medium,
            SamplerPolicy::ClampHigh => &self.clamp_high,
            SamplerPolicy::SkyLow => &self.sky_low,
            SamplerPolicy::SkyMedium => &self.sky_medium,
            SamplerPolicy::SkyHigh => &self.sky_high,
            SamplerPolicy::RepeatNearest => &self.repeat_nearest,
            SamplerPolicy::ClampNearest => &self.clamp_nearest,
            SamplerPolicy::ClampLinear => &self.clamp_linear,
        }
    }
}

/// What one cache lookup did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheOutcome {
    /// The texture was decoded, fitted, mip-generated and uploaded.
    Uploaded,
    /// The renderer already held this exact semantic texture.
    Reused,
}

/// One texture a [`TextureCache::get_or_upload_batch`] call must make resident.
///
/// The batch counterpart of a single upload call: the caller builds the full
/// semantic key ([`TextureKey::new`] for a resolved material texture, or the
/// literal fields for a clamped fixture/prop sheet — logical id, semantic,
/// class, level and [`TextureWrap::Clamp`], exactly like
/// [`TextureCache::get_or_upload_fitted`]), names the decoded source image and
/// the origin that selects the cache lifetime.
pub struct TextureUploadRequest<'a> {
    /// The full semantic identity; the batch reuses it verbatim.
    pub key: TextureKey,
    /// The decoded source image the fit and the mip chain run on.
    pub image: &'a RawImage,
    /// Where the image came from; also its cache lifetime.
    pub origin: TextureOrigin,
}

/// Prepares every request's CPU data in parallel, one slot per request.
///
/// The workers borrow only the decoded [`RawImage`]s and the key's fit
/// parameters; `Arc` handles, cache maps and GPU state never cross a thread.
/// Unprepared slots remain `None` after a spawn failure or worker panic; the
/// caller falls back to the identical serial preparation for those requests.
fn prepare_textures(requests: &[&TextureUploadRequest<'_>]) -> Vec<Option<PreparedTexture>> {
    let jobs = requests.len();
    let slots: Vec<Option<PreparedTexture>> = (0..jobs).map(|_| None).collect();
    if jobs == 0 {
        return slots;
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .clamp(1, MAX_TEXTURE_WORKERS)
        .min(jobs);
    let next = AtomicUsize::new(0);
    let shared_slots = Mutex::new(slots);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let spawned = std::thread::Builder::new().spawn_scoped(scope, || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= jobs {
                        break;
                    }
                    let Some(request) = requests.get(index) else {
                        break;
                    };
                    let prepared = PreparedTexture::prepare(
                        request.image,
                        request.key.level,
                        request.key.class,
                        request.key.semantic,
                    );
                    if let Ok(mut locked_slots) = shared_slots.lock()
                        && let Some(slot) = locked_slots.get_mut(index)
                    {
                        *slot = Some(prepared);
                    }
                }
            });
            match spawned {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    crate::engine::logging::warn(format!(
                        "[textures] could not start preparation worker: {error}; using serial fallback"
                    ));
                    break;
                }
            }
        }
        for handle in handles {
            if let Err(payload) = handle.join() {
                crate::engine::logging::warn(format!(
                    "[textures] preparation worker panicked: {}; using serial fallback",
                    crate::engine::logging::panic_message(payload.as_ref())
                ));
            }
        }
    });
    shared_slots
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The renderer-owned cache from semantic texture identity to GPU textures.
///
/// Two retention policies:
///
/// * catalog and diagnostic textures retain bounded prior-upload data, so
///   switching back can reuse them;
/// * pack textures (`TextureOrigin::Pack`) are dropped at the next level
///   upload, because a pack is owned by one level.
///
/// The fallback sheet, the bind group layout and the samplers are created once
/// and never dropped until the renderer is.
#[derive(Clone)]
pub struct TextureCache {
    layout: wgpu::BindGroupLayout,
    samplers: Samplers,
    /// Whether the adapter filters anisotropically; the world presets clamp
    /// their request to 1x when it does not.
    anisotropy_supported: bool,
    persistent: HashMap<TextureKey, Arc<GpuTexture>>,
    level: HashMap<TextureKey, Arc<GpuTexture>>,
    fallback: Arc<GpuTexture>,
}

impl TextureCache {
    /// Creates the layout, the shared samplers and the fallback sheet.
    ///
    /// `anisotropy_supported` is the adapter's
    /// `DownlevelFlags::ANISOTROPIC_FILTERING` capability: when it is false the
    /// world presets are created with their anisotropy request clamped to 1
    /// (their linear/linear/linear filtering is unchanged). GPU work at
    /// construction is exactly one 2x2 upload; every level-scoped texture is
    /// created later, at that level's upload.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, anisotropy_supported: bool) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("places-wgpu-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let samplers = Samplers::new(device, anisotropy_supported);
        let fallback = Arc::new(GpuTexture::upload_prepared(
            device,
            queue,
            &layout,
            &samplers,
            &TextureUpload {
                key: &TextureKey {
                    logical: FALLBACK_TEXTURE_KEY.to_string(),
                    semantic: TextureSemantic::BaseColorDisplay,
                    class: TextureClass::Surface,
                    // The fallback sheet is never downscaled, so it is
                    // level-independent; the key only records which level's
                    // metadata describes it.
                    level: QualityLevel::DEFAULT,
                    wrap: TextureWrap::Repeat,
                },
                prepared: &PreparedTexture::single(fallback_white_image()),
                origin: TextureOrigin::Catalog,
                fallback: true,
            },
        ));
        Self {
            layout,
            samplers,
            anisotropy_supported,
            persistent: HashMap::new(),
            level: HashMap::new(),
            fallback,
        }
    }

    /// Whether the adapter filters anisotropically.
    ///
    /// False means the three world presets were created with their anisotropy
    /// request clamped to 1x; the startup diagnostic reports it once.
    #[must_use]
    pub const fn anisotropy_supported(&self) -> bool {
        self.anisotropy_supported
    }

    /// The texture + sampler bind group layout (pipeline group 1).
    ///
    /// Created once and shared by every world pipeline rebuild, so a changed
    /// surface format never invalidates a cached bind group.
    #[must_use]
    pub const fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    /// The shared fallback sheet, for a draw with no base texture.
    #[must_use]
    pub fn fallback(&self) -> Arc<GpuTexture> {
        Arc::clone(&self.fallback)
    }

    /// One shared sampler by policy, for a caller that builds a bind group
    /// against a different layout (a material's normal-map binding).
    #[must_use]
    pub const fn sampler(&self, policy: SamplerPolicy) -> &wgpu::Sampler {
        self.samplers.get(policy)
    }

    /// Drops the previous level's pack textures.
    ///
    /// Called once per level upload, before the new level resolves its
    /// textures; prior catalog entries are retained within a bounded budget.
    pub fn begin_level(&mut self) {
        self.level.clear();
        self.trim_retained(256, 256 * 1024 * 1024);
    }

    /// Bounds old cache references before upload, never between draws or
    /// sheets of the same upload. Active worlds retain independent Arcs.
    fn trim_retained(&mut self, max_entries: usize, max_bytes: u64) {
        let mut bytes = self.persistent.values().fold(0_u64, |total, image| {
            total.saturating_add(image.meta().resident_bytes)
        });
        if self.persistent.len() <= max_entries && bytes <= max_bytes {
            return;
        }
        let mut candidates: Vec<_> = self
            .persistent
            .iter()
            .map(|(key, image)| (key.clone(), image.meta().resident_bytes))
            .collect();
        // Debug identity is only an eviction tie-break, never a texture key.
        candidates.sort_by_cached_key(|(key, size)| (std::cmp::Reverse(*size), format!("{key:?}")));
        for (key, size) in candidates {
            if self.persistent.len() <= max_entries && bytes <= max_bytes {
                break;
            }
            drop(self.persistent.remove(&key));
            bytes = bytes.saturating_sub(size);
        }
    }

    /// Drops every profile-fitted texture.
    ///
    /// The quality profile is part of a texture's identity, so a profile change
    /// releases the old fits; the fallback sheet and the layout survive. The
    /// loaded level's draws keep their `Arc`s alive until the rebuild replaces
    /// them, so no frame can bind a freed texture.
    pub fn release_profile_textures(&mut self) {
        self.persistent.clear();
        self.level.clear();
    }

    /// Cache-owned texel allocations, deduplicated by shared handle. Active
    /// draws can also retain these allocations; admission charges overlap
    /// conservatively instead of treating a cache reference as free memory.
    pub fn retained_texel_bytes(&self) -> u64 {
        let mut seen = std::collections::HashSet::new();
        self.persistent
            .values()
            .chain(self.level.values())
            .fold(0_u64, |bytes, texture| {
                if seen.insert(Arc::as_ptr(texture)) {
                    bytes.saturating_add(texture.meta().resident_bytes)
                } else {
                    bytes
                }
            })
    }

    /// Cache-owned references only: active/pending draws can retain additional
    /// sheets, and a cloned cache can share the same underlying allocations.
    #[cfg(feature = "visual-diagnostics")]
    pub fn diagnostic_retention(&self) -> serde_json::Value {
        let bytes = |entries: &HashMap<TextureKey, Arc<GpuTexture>>| {
            entries.values().fold(0_u64, |total, image| {
                total.saturating_add(image.meta().resident_bytes)
            })
        };
        serde_json::json!({
            "persistent_entries": self.persistent.len(),
            "persistent_texel_bytes": bytes(&self.persistent),
            "level_entries": self.level.len(),
            "level_texel_bytes": bytes(&self.level),
        })
    }

    /// Returns the cached GPU texture for one resolved texture, uploading it
    /// (fit, mip chain, view, bind groups) when it is not cached yet.
    ///
    /// The key is the semantic identity, so fifty surfaces referencing one
    /// texture upload once; the caller only has to ask for the same
    /// [`ResolvedTexture`] under the same semantic and level.
    #[must_use]
    pub fn get_or_upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolved: &ResolvedTexture,
        semantic: TextureSemantic,
        level: QualityLevel,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        let key = TextureKey::new(resolved, semantic, level);
        self.get_or_upload_key(device, queue, key, resolved.image.as_ref(), resolved.origin)
    }

    /// Uploads one fitted, clamped single-use sheet under a logical key.
    ///
    /// The prop and fixture sheets are not part of the level's material table:
    /// a model's own PNG and a fixture family's face arrive as decoded
    /// [`RawImage`]s with their own session key. Their GPU identity is the same
    /// kind of cache entry, clamped because fitted UVs never repeat.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "one upload entry point: device/queue plus the fitted sheet's identity"
    )] // one upload entry point: device/queue plus the fitted sheet's identity
    pub fn get_or_upload_fitted(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        logical: &str,
        image: &RawImage,
        class: TextureClass,
        origin: TextureOrigin,
        level: QualityLevel,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        let key = TextureKey {
            logical: logical.to_string(),
            semantic: TextureSemantic::BaseColorDisplay,
            class,
            level,
            wrap: TextureWrap::Clamp,
        };
        self.get_or_upload_key(device, queue, key, image, origin)
    }

    /// Uploads one level sheet under an explicit cache identity.
    ///
    /// Used by the sky, whose equirectangular sheet repeats in U (the seam) and
    /// clamps in V (the poles): the caller builds the full [`TextureKey`] and
    /// this is the one upload path, so a bespoke identity cannot bypass the
    /// cache's retirement rules.
    pub fn get_or_upload_with_key(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: TextureKey,
        image: &RawImage,
        origin: TextureOrigin,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        self.get_or_upload_key(device, queue, key, image, origin)
    }

    /// Resolves many textures at once: cache hits immediately, the CPU
    /// preparation of the misses in parallel, then one serial upload pass.
    ///
    /// The returned entries are aligned with `requests`, in request order. The
    /// heavy CPU work (the quality fit and the box-filtered mip chain) is the
    /// part that parallelizes; device object creation and
    /// `Queue::write_texture` run here on the caller's thread, so no GPU handle
    /// or queue is ever shared. Results are byte-identical to calling
    /// [`Self::get_or_upload`]/[`Self::get_or_upload_fitted`] once per distinct
    /// request in order, and a duplicate key inside one batch resolves like the
    /// serial path: the first occurrence may upload, later occurrences reuse it.
    #[must_use]
    pub fn get_or_upload_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        requests: &[TextureUploadRequest<'_>],
    ) -> Vec<(CacheOutcome, Arc<GpuTexture>)> {
        enum Plan {
            /// Already resident: the cache handed out its `Arc`.
            Resident(Arc<GpuTexture>),
            /// A miss; the index of its entry in `misses`.
            Miss(usize),
            /// A later occurrence of a miss in this batch; the same index.
            Duplicate(usize),
        }

        let mut plans: Vec<Plan> = Vec::with_capacity(requests.len());
        let mut misses: Vec<&TextureUploadRequest<'_>> = Vec::new();
        // Identity includes the lifetime: the same key can be a catalog entry
        // and a pack entry, and those are two cache entries.
        let mut first_pending: HashMap<(&TextureKey, bool), usize> = HashMap::new();
        for request in requests {
            self.retire_prior_revision(&request.key, request.origin);
            if let Some(texture) = self.get(&request.key, request.origin).map(Arc::clone) {
                plans.push(Plan::Resident(texture));
                continue;
            }
            let identity = (&request.key, request.origin == TextureOrigin::Pack);
            if let Some(&slot) = first_pending.get(&identity) {
                plans.push(Plan::Duplicate(slot));
                continue;
            }
            let slot = misses.len();
            let _previous_value = first_pending.insert(identity, slot);
            misses.push(request);
            plans.push(Plan::Miss(slot));
        }

        let mut uploaded: Vec<(CacheOutcome, Arc<GpuTexture>)> = Vec::with_capacity(misses.len());
        // Eight maximum-sized world sheets bound the prepared mip staging to
        // about 43 MiB. Upload/drop each group before preparing the next; the
        // request plan preserves duplicate/cache semantics across groups.
        for group in misses.chunks(8) {
            let prepared = prepare_textures(group);
            for (request, slot) in group.iter().zip(prepared) {
                // A worker that never filled its slot (only possible if it
                // panicked, which `thread::scope` would have propagated) falls back
                // to the identical serial preparation rather than failing.
                let prepared_texture = slot.unwrap_or_else(|| {
                    PreparedTexture::prepare(
                        request.image,
                        request.key.level,
                        request.key.class,
                        request.key.semantic,
                    )
                });
                let texture = Arc::new(GpuTexture::upload_prepared(
                    device,
                    queue,
                    &self.layout,
                    &self.samplers,
                    &TextureUpload {
                        key: &request.key,
                        prepared: &prepared_texture,
                        origin: request.origin,
                        fallback: false,
                    },
                ));
                drop(
                    self.map_mut(request.origin)
                        .insert(request.key.clone(), Arc::clone(&texture)),
                );
                uploaded.push((CacheOutcome::Uploaded, texture));
            }
        }
        crate::engine::perf::startup_mark("textures: CPU prepare/GPU upload");

        plans
            .into_iter()
            .map(|plan| match plan {
                Plan::Resident(texture) => (CacheOutcome::Reused, texture),
                // A miss uploaded in this batch reports its upload; a later
                // occurrence of the same identity is a reuse, exactly like the
                // serial path.
                Plan::Miss(slot) => uploaded.get(slot).map_or_else(
                    || (CacheOutcome::Reused, self.fallback()),
                    |entry| (entry.0, Arc::clone(&entry.1)),
                ),
                Plan::Duplicate(slot) => uploaded.get(slot).map_or_else(
                    || (CacheOutcome::Reused, self.fallback()),
                    |entry| (CacheOutcome::Reused, Arc::clone(&entry.1)),
                ),
            })
            .collect()
    }

    /// The shared body of both single-texture upload entry points.
    ///
    /// The key carries the class and level the fit needs, so no caller can
    /// pass values that disagree with the cache identity.
    fn get_or_upload_key(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: TextureKey,
        image: &RawImage,
        origin: TextureOrigin,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        self.retire_prior_revision(&key, origin);
        if let Some(texture) = self.get(&key, origin).map(Arc::clone) {
            return (CacheOutcome::Reused, texture);
        }
        let prepared = PreparedTexture::prepare(image, key.level, key.class, key.semantic);
        let texture = Arc::new(GpuTexture::upload_prepared(
            device,
            queue,
            &self.layout,
            &self.samplers,
            &TextureUpload {
                key: &key,
                prepared: &prepared,
                origin,
                fallback: false,
            },
        ));
        drop(self.map_mut(origin).insert(key, Arc::clone(&texture)));
        (CacheOutcome::Uploaded, texture)
    }

    /// Cache eviction never invalidates the Arcs held by a resident world.
    fn retire_prior_revision(&mut self, key: &TextureKey, origin: TextureOrigin) {
        let Some(source) = revision_source(&key.logical) else {
            return;
        };
        self.map_mut(origin).retain(|stored, _| {
            stored.logical == key.logical
                || revision_source(&stored.logical).is_none_or(|previous| previous != source)
        });
    }

    /// The cached entry for a key under the lifetime its origin implies.
    fn get(&self, key: &TextureKey, origin: TextureOrigin) -> Option<&Arc<GpuTexture>> {
        if origin == TextureOrigin::Pack {
            self.level.get(key)
        } else {
            self.persistent.get(key)
        }
    }

    /// The map a key of this origin belongs to.
    fn map_mut(&mut self, origin: TextureOrigin) -> &mut HashMap<TextureKey, Arc<GpuTexture>> {
        if origin == TextureOrigin::Pack {
            &mut self.level
        } else {
            &mut self.persistent
        }
    }
}

/// The committed white sheet, with an embedded 2x2 PNG of the original
/// emergency white pixels if the primary repository image cannot decode.
#[must_use]
pub fn fallback_white_image() -> RawImage {
    decode_png(FALLBACK_WHITE_PNG)
        .unwrap_or_else(|_| crate::engine::materials::BuiltinImage::EmergencyWhite.decode())
}

/// Mip levels a full chain of this image holds.
///
/// `floor(log2(longest edge)) + 1`: 1024x1024 -> 11, 2x2 -> 2, 1x1 -> 1,
/// 96x64 -> 7, 3x3 -> 2. The chain stops at 1x1.
#[must_use]
pub const fn mip_level_count(width: u32, height: u32) -> u32 {
    let longest = if width > height { width } else { height };
    if longest == 0 {
        return 1;
    }
    u32::BITS.saturating_sub(longest.leading_zeros())
}

/// Bytes in one tightly packed RGBA8 row.
///
/// The upload path is `Queue::write_texture`, which accepts unpadded rows (it
/// stages the copy internally); this is the row pitch the uploads declare, and
/// no 256-byte padding is required or applied. A test covers the 96x64 sheet,
/// whose 384-byte rows are not 256-aligned.
#[must_use]
pub const fn row_bytes(width: u32) -> u32 {
    width.saturating_mul(RGBA_BYTES)
}

/// Bytes one RGBA8 level of these dimensions occupies.
fn level_bytes(width: u32, height: u32) -> u64 {
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(u64::from(RGBA_BYTES))
}

/// Box-filters an image to half its size, rounding to the nearest channel
/// value and clamping at the edges for odd dimensions.
///
/// Numeric-data box filter. Colour uses the separate linear-light filter.
/// Every output texel
/// is the average of its 2x2 source block (fewer texels at an odd edge), so a
/// power-of-two source produces the exact successive averages the reference
/// generates. Alpha is averaged the same way and never discarded.
///
/// This is the startup path's hot loop (a full CPU mip chain per uploaded
/// sheet), so it works on whole source rows: one bounds check per row and
/// per output texel instead of an `Option`-returning lookup per source texel.
/// The arithmetic is the same `(sum + count / 2) / count` average as before,
/// and a short or ragged buffer yields exactly the same zeros the per-texel
/// lookup produced.
#[must_use]
fn halve_image(source: &RawImage) -> RawImage {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let Some(length) = buffer_len(width, height) else {
        return RawImage::new(1, 1, vec![255, 255, 255, 255]);
    };
    let mut rgba = vec![0u8; length];

    // Row pitches as `usize`, saturating at the (impossible for a real image)
    // conversion limit. The checked `buffer_len` above already proved the
    // output buffer fits; the source pitch is only used to slice.
    let source_width = usize::try_from(source.width).unwrap_or(usize::MAX);
    let source_height = usize::try_from(source.height).unwrap_or(usize::MAX);
    let source_stride = source_width.saturating_mul(RGBA_CHANNELS);
    // A zero-width source still contributed one texel per row at offset zero
    // in the reference arithmetic, so the row window never shrinks below one
    // texel; a real image's stride is already at least that.
    let source_window = source_stride.max(RGBA_CHANNELS);
    let out_width = usize::try_from(width).unwrap_or(usize::MAX);
    let out_height = usize::try_from(height).unwrap_or(usize::MAX);
    let out_stride = out_width.saturating_mul(RGBA_CHANNELS);

    for out_y in 0..out_height {
        let out_start = out_y.saturating_mul(out_stride);
        let Some(out_row) = rgba.get_mut(out_start..out_start.saturating_add(out_stride)) else {
            continue;
        };
        let out_texels = out_row.as_chunks_mut::<RGBA_CHANNELS>().0;
        let y0 = out_y.saturating_mul(2);
        let y1 = y0.saturating_add(2).min(source_height.max(1));
        for out_x in 0..out_width {
            let x0 = out_x.saturating_mul(2);
            let x1 = x0.saturating_add(2).min(source_width.max(1));
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                let row_start = y.saturating_mul(source_stride);
                let row = source.rgba.get(row_start..).map_or(&[][..], |tail| {
                    let end = tail.len().min(source_window);
                    tail.get(..end).unwrap_or_default()
                });
                let texels = row.as_chunks::<RGBA_CHANNELS>().0;
                let Some(block) = texels.get(x0..x1.min(texels.len())) else {
                    continue;
                };
                for texel in block {
                    let [red, green, blue, alpha] = *texel;
                    sums[0] = sums[0].saturating_add(u32::from(red));
                    sums[1] = sums[1].saturating_add(u32::from(green));
                    sums[2] = sums[2].saturating_add(u32::from(blue));
                    sums[3] = sums[3].saturating_add(u32::from(alpha));
                    count = count.saturating_add(1);
                }
            }
            if count == 0 {
                continue;
            }
            let Some(slot) = out_texels.get_mut(out_x) else {
                continue;
            };
            // `sum + count / 2` with `count <= 4` and a channel sum <= 4 * 255
            // can never saturate or divide by zero; the checked forms are the
            // same arithmetic the exact reference implementation used.
            let round = |sum: u32| -> u8 {
                let rounded = sum
                    .saturating_add(count / 2)
                    .checked_div(count)
                    .unwrap_or(0)
                    .min(u32::from(u8::MAX));
                u8::try_from(rounded).unwrap_or(u8::MAX)
            };
            let [red, green, blue, alpha] = sums;
            *slot = [round(red), round(green), round(blue), round(alpha)];
        }
    }
    RawImage::new(width, height, rgba)
}

/// Byte length of an RGBA8 buffer, or `None` on overflow.
fn buffer_len(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(usize::try_from(RGBA_BYTES).ok()?)
}

/// Writes one mip level through the queue.
///
/// `Queue::write_texture` stages the data internally and accepts a tightly
/// packed `bytes_per_row`; the 256-byte row alignment that
/// `copy_buffer_to_texture` requires does not apply here.
fn write_mip(queue: &wgpu::Queue, texture: &wgpu::Texture, level: u32, image: &RawImage) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &image.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(row_bytes(image.width)),
            rows_per_image: Some(image.height),
        },
        wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
    );
}

/// Creates one texture + sampler bind group.
fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("places-wgpu-texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

mod worker;
pub use worker::{PreparedImage, TextureJob, TextureSource};

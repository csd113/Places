//! Stage 6 texture infrastructure: Places image assets as wgpu GPU textures.
//!
//! The engine hands this module a decoded [`RawImage`] under the same logical
//! texture identity the OpenGL reference renderer uses; this module owns
//! everything after that: the GPU texture, its mip chain, its view, the shared
//! samplers and the one texture + sampler bind group a draw binds.
//!
//! Scope is deliberately the ordinary base-colour path:
//!
//! * base-colour images upload as raw `Rgba8Unorm` display values, exactly like
//!   the reference's non-sRGB `GL_RGBA` sheets: filtering, blending and mip
//!   selection all happen in the reference's display space, and the sRGB
//!   surface is the single conversion point. (Stage 6-9 sampled
//!   `Rgba8UnormSrgb` and re-encoded in the shader; Stage 10 measured the
//!   decode/blend round trip as a convexity bias and returned to raw.) The
//!   semantic enum still distinguishes colour from linear data textures
//!   (normal maps, masks);
//! * mips are generated on the CPU by a deterministic 2x2 box filter over the
//!   raw 8-bit channels, the same arithmetic the OpenGL reference's
//!   `glGenerateMipmap` applies to its non-sRGB `GL_RGBA` textures;
//! * the player's filtering setting selects between two shared repeating
//!   samplers (linear/linear/linear and nearest/nearest/nearest, exactly the
//!   reference's `LINEAR_MIPMAP_LINEAR`/`LINEAR` and
//!   `NEAREST_MIPMAP_NEAREST`/`NEAREST`); the fallback sheet uses the
//!   reference's clamped nearest sampler, with no mip chain;
//! * nothing else: normal maps, lightmaps, emissive masks, decals, props and
//!   render targets are later stages and never reach this module in Stage 6.
//!
//! The cache is keyed by semantic identity (logical texture id + colour
//! interpretation + quality class + profile), never by material instance or
//! draw index, so every surface that references one texture shares one GPU
//! upload. Catalog and diagnostic textures live for the renderer's lifetime;
//! pack textures live for one level, exactly like the reference's caches.
//! There is no eviction beyond those two lifetimes and no LRU: the shipped
//! catalog holds 38 texture assets in total, so a renderer-lifetime map is
//! bounded by the catalog plus the missing-texture pattern.

use std::collections::HashMap;
use std::sync::Arc;

use crate::materials::{RawImage, ResolvedTexture, TextureOrigin, decode_png};
use crate::quality::{QualityProfile, TextureClass, fit_image};

/// Bytes per RGBA8 texel.
const RGBA_BYTES: u32 = 4;

/// Bytes of the generated fallback of last resort (2x2 RGBA8).
const FALLBACK_BYTES: usize = 16;

/// The shared untextured fallback, as committed artwork.
///
/// The same `assets/core/textures/white_01.png` the reference renderer loaded
/// at startup (and embedded for an assets-less install): a 2x2 opaque white
/// sheet, sampled clamped and nearest, with no mip chain. It is a real
/// repository asset, not a generated fill; the decode below is the only path
/// that turns it into pixels.
const FALLBACK_WHITE_PNG: &[u8] = include_bytes!("../../../assets/core/textures/white_01.png");

/// The logical id of that sheet in the catalog.
const FALLBACK_TEXTURE_KEY: &str = "core:tex_white_01";

/// How one GPU texture's channels are meant to be interpreted.
///
/// Stage 6 created only [`Self::BaseColorDisplay`]; Stage 7 adds the first
/// [`Self::DataLinear`] textures (material normal maps), whose cache key and
/// format the variant already pins.
///
/// Stage 10 corrected [`Self::BaseColorDisplay`] from `Rgba8UnormSrgb` to raw
/// `Rgba8Unorm`: the reference has no sRGB anywhere, so it filters, blends and
/// mips authored bytes in display space. Sampling an sRGB copy made the
/// hardware decode each filtered blend and the shader re-encode it — a
/// convexity bias measured as a broad +1 display level on minified surfaces.
/// Returning the texture to raw improved every canonical view; the exact
/// per-texel decode/encode round trip itself is proven exact by
/// `the_srgb_sample_round_trip_is_measured_on_this_adapter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureSemantic {
    /// Authored colour: 8-bit RGBA display values, sampled as authored.
    BaseColorDisplay,
    /// Numeric data (a normal map, a mask, a lightmap). Also sampled raw.
    DataLinear,
}

impl TextureSemantic {
    /// The wgpu format this semantic uploads as.
    ///
    /// Both are raw `Rgba8Unorm`: the colour targets are display-space, so
    /// their source textures must be too, and numeric data is never
    /// colour-encoded. The one display-to-surface conversion is the world
    /// fragment stage's `srgb_to_linear` at the sRGB surface, exactly where
    /// the reference's presentation happened.
    #[must_use]
    pub const fn format(self) -> wgpu::TextureFormat {
        match self {
            Self::BaseColorDisplay | Self::DataLinear => wgpu::TextureFormat::Rgba8Unorm,
        }
    }
}

/// How one GPU texture's UVs are addressed.
///
/// Tiling surface sheets repeat; a fitted single-use sheet (a prop model's own
/// texture, a fixture face) clamps, because its UVs never leave the image and a
/// repeat wrap would bleed one edge of the artwork into the opposite edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureWrap {
    /// `REPEAT` addressing.
    Repeat,
    /// `CLAMP_TO_EDGE` addressing.
    Clamp,
}

impl TextureWrap {
    /// The sampler policy for one filtering mode at this wrap.
    #[must_use]
    pub const fn policy(self, filtering: TextureFiltering) -> SamplerPolicy {
        match (self, filtering) {
            (Self::Repeat, TextureFiltering::Linear) => SamplerPolicy::RepeatLinear,
            (Self::Repeat, TextureFiltering::Nearest) => SamplerPolicy::RepeatNearest,
            (Self::Clamp, TextureFiltering::Linear) => SamplerPolicy::ClampLinear,
            (Self::Clamp, TextureFiltering::Nearest) => SamplerPolicy::ClampNearest,
        }
    }
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
    /// The active quality profile (the fit's edge budget).
    pub profile: QualityProfile,
    /// How the UVs are addressed.
    pub wrap: TextureWrap,
}

impl TextureKey {
    /// A key for one resolved texture at one semantic and profile, repeating.
    #[must_use]
    pub fn new(
        resolved: &ResolvedTexture,
        semantic: TextureSemantic,
        profile: QualityProfile,
    ) -> Self {
        Self {
            logical: resolved.key.clone(),
            semantic,
            class: resolved.class,
            profile,
            wrap: TextureWrap::Repeat,
        }
    }
}
/// Which sampler the frame's filtering setting selects.
///
/// This mirrors the OpenGL reference's `set_repeat_filter`: the setting is
/// global, so the world shares one sampler per filtering mode rather than one
/// sampler per texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureFiltering {
    /// `LINEAR_MIPMAP_LINEAR` minification, `LINEAR` magnification.
    #[default]
    Linear,
    /// `NEAREST_MIPMAP_NEAREST` minification, `NEAREST` magnification.
    Nearest,
}

impl TextureFiltering {
    /// Parses the player-facing setting. Any value but `"nearest"` is linear,
    /// exactly like the reference's parser (`mode != "nearest"`); the settings
    /// loader validates the two accepted names first.
    #[must_use]
    pub fn parse(mode: &str) -> Self {
        if mode == "nearest" {
            Self::Nearest
        } else {
            Self::Linear
        }
    }

    /// Stable name for diagnostics.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Nearest => "nearest",
        }
    }

    /// The sampler policy this filtering mode selects for repeating textures.
    #[must_use]
    pub const fn sampler_policy(self) -> SamplerPolicy {
        match self {
            Self::Linear => SamplerPolicy::RepeatLinear,
            Self::Nearest => SamplerPolicy::RepeatNearest,
        }
    }

    /// The sampler policy this filtering mode selects for a clamped single-use
    /// sheet (a prop model's own sheet, a fixture face, an emissive mask).
    #[must_use]
    pub const fn clamp_sampler_policy(self) -> SamplerPolicy {
        match self {
            Self::Linear => SamplerPolicy::ClampLinear,
            Self::Nearest => SamplerPolicy::ClampNearest,
        }
    }
}

/// One shared sampler configuration. Only the four Stage 6/9 uses exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SamplerPolicy {
    /// A repeating tiling sheet with linear min/mag/mip filtering.
    RepeatLinear,
    /// The same sheet under the nearest filtering setting.
    RepeatNearest,
    /// A clamped, unfiltered single-use sheet (the shared fallback).
    ClampNearest,
    /// A clamped, linear-filtered single-use sheet: a prop model's own sheet, a
    /// fixture face, at the player's linear setting. The reference uploads
    /// those `CLAMP_TO_EDGE` with a mip chain, exactly like this policy.
    ClampLinear,
}

impl SamplerPolicy {
    /// Stable label, used for the GPU object and the tests.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RepeatLinear => "places-wgpu-sampler-repeat-linear",
            Self::RepeatNearest => "places-wgpu-sampler-repeat-nearest",
            Self::ClampNearest => "places-wgpu-sampler-clamp-nearest",
            Self::ClampLinear => "places-wgpu-sampler-clamp-linear",
        }
    }

    /// The full descriptor, every field explicit: no wgpu default is relied on.
    #[must_use]
    pub const fn descriptor(self) -> wgpu::SamplerDescriptor<'static> {
        let (address, filter, mip) = match self {
            Self::RepeatLinear => (
                wgpu::AddressMode::Repeat,
                wgpu::FilterMode::Linear,
                wgpu::MipmapFilterMode::Linear,
            ),
            Self::RepeatNearest => (
                wgpu::AddressMode::Repeat,
                wgpu::FilterMode::Nearest,
                wgpu::MipmapFilterMode::Nearest,
            ),
            Self::ClampNearest => (
                wgpu::AddressMode::ClampToEdge,
                wgpu::FilterMode::Nearest,
                wgpu::MipmapFilterMode::Nearest,
            ),
            Self::ClampLinear => (
                wgpu::AddressMode::ClampToEdge,
                wgpu::FilterMode::Linear,
                wgpu::MipmapFilterMode::Linear,
            ),
        };
        wgpu::SamplerDescriptor {
            label: Some(self.label()),
            address_mode_u: address,
            address_mode_v: address,
            address_mode_w: address,
            mag_filter: filter,
            min_filter: filter,
            mipmap_filter: mip,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        }
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
    /// The profile the image was fitted for.
    pub profile: QualityProfile,
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
    /// Texture + repeating linear sampler.
    linear_bind_group: wgpu::BindGroup,
    /// Texture + repeating nearest sampler.
    nearest_bind_group: wgpu::BindGroup,
    meta: TextureMeta,
}

impl GpuTexture {
    /// Uploads one fitted image with a CPU-generated mip chain.
    ///
    /// `mip_levels` is explicit: ordinary sheets get
    /// [`mip_level_count`] levels, the fallback sheet gets exactly one (the
    /// reference never mip-chains it). Every allocated level is written before
    /// the function returns, so no level can be sampled uninitialized.
    fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        samplers: &Samplers,
        upload: &TextureUpload<'_>,
    ) -> Self {
        let image = upload.image;
        let format = upload.key.semantic.format();
        let mip_levels = upload.mip_levels.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("places-wgpu-texture"),
            size: wgpu::Extent3d {
                width: image.width.max(1),
                height: image.height.max(1),
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

        let mut resident_bytes = level_bytes(image.width, image.height);
        write_mip(queue, &texture, 0, image);
        let mut current = image.clone();
        for level in 1..mip_levels {
            current = halve_image(&current);
            resident_bytes =
                resident_bytes.saturating_add(level_bytes(current.width, current.height));
            write_mip(queue, &texture, level, &current);
        }

        // The fallback sheet keeps the reference's clamped nearest sampler in
        // both slots: it is a solid fill, so the two filtering modes cannot
        // disagree about a texel. Every other texture follows its own wrap
        // contract and the player's filtering setting.
        let (linear, nearest) = if upload.fallback {
            let clamp = samplers.get(SamplerPolicy::ClampNearest);
            let group = create_bind_group(device, layout, &view, clamp);
            (group, create_bind_group(device, layout, &view, clamp))
        } else {
            (
                create_bind_group(
                    device,
                    layout,
                    &view,
                    samplers.get(upload.key.wrap.policy(TextureFiltering::Linear)),
                ),
                create_bind_group(
                    device,
                    layout,
                    &view,
                    samplers.get(upload.key.wrap.policy(TextureFiltering::Nearest)),
                ),
            )
        };

        Self {
            _texture: texture,
            view,
            linear_bind_group: linear,
            nearest_bind_group: nearest,
            meta: TextureMeta {
                width: image.width,
                height: image.height,
                mip_levels,
                format,
                semantic: upload.key.semantic,
                class: upload.key.class,
                profile: upload.key.profile,
                origin: upload.origin,
                fallback: upload.fallback,
                resident_bytes,
            },
        }
    }

    /// The bind group for one filtering mode.
    #[must_use]
    pub const fn bind_group(&self, filtering: TextureFiltering) -> &wgpu::BindGroup {
        match filtering {
            TextureFiltering::Linear => &self.linear_bind_group,
            TextureFiltering::Nearest => &self.nearest_bind_group,
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
    image: &'a RawImage,
    mip_levels: u32,
    origin: TextureOrigin,
    fallback: bool,
}

/// The four shared samplers the Stage 6/9 world uses.
struct Samplers {
    repeat_linear: wgpu::Sampler,
    repeat_nearest: wgpu::Sampler,
    clamp_nearest: wgpu::Sampler,
    clamp_linear: wgpu::Sampler,
}

impl Samplers {
    /// Creates the shared samplers once per device.
    fn new(device: &wgpu::Device) -> Self {
        let create = |policy: SamplerPolicy| device.create_sampler(&policy.descriptor());
        Self {
            repeat_linear: create(SamplerPolicy::RepeatLinear),
            repeat_nearest: create(SamplerPolicy::RepeatNearest),
            clamp_nearest: create(SamplerPolicy::ClampNearest),
            clamp_linear: create(SamplerPolicy::ClampLinear),
        }
    }

    /// The sampler for a policy.
    const fn get(&self, policy: SamplerPolicy) -> &wgpu::Sampler {
        match policy {
            SamplerPolicy::RepeatLinear => &self.repeat_linear,
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

/// The renderer-owned cache from semantic texture identity to GPU textures.
///
/// Two lifetimes, matching the reference renderer's GL caches:
///
/// * catalog and diagnostic textures persist for the renderer's lifetime, so
///   switching back to a level reuses them;
/// * pack textures (`TextureOrigin::Pack`) are dropped at the next level
///   upload, because a pack is owned by one level.
///
/// The fallback sheet, the bind group layout and the samplers are created once
/// and never dropped until the renderer is.
pub struct TextureCache {
    layout: wgpu::BindGroupLayout,
    samplers: Samplers,
    persistent: HashMap<TextureKey, Arc<GpuTexture>>,
    level: HashMap<TextureKey, Arc<GpuTexture>>,
    fallback: Arc<GpuTexture>,
}

impl TextureCache {
    /// Creates the layout, the shared samplers and the fallback sheet.
    ///
    /// GPU work at construction is exactly one 2x2 upload; every level-scoped
    /// texture is created later, at that level's upload.
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
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
        let samplers = Samplers::new(device);
        let fallback = Arc::new(GpuTexture::upload(
            device,
            queue,
            &layout,
            &samplers,
            &TextureUpload {
                key: &TextureKey {
                    logical: FALLBACK_TEXTURE_KEY.to_string(),
                    semantic: TextureSemantic::BaseColorDisplay,
                    class: TextureClass::Surface,
                    profile: QualityProfile::Full,
                    wrap: TextureWrap::Repeat,
                },
                image: &fallback_white_image(),
                mip_levels: 1,
                origin: TextureOrigin::Catalog,
                fallback: true,
            },
        ));
        Self {
            layout,
            samplers,
            persistent: HashMap::new(),
            level: HashMap::new(),
            fallback,
        }
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
    /// textures; catalog entries survive, exactly like the reference's
    /// persistent caches.
    pub fn begin_level(&mut self) {
        self.level.clear();
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

    /// Returns the cached GPU texture for one resolved texture, uploading it
    /// (fit, mip chain, view, bind groups) when it is not cached yet.
    ///
    /// The key is the semantic identity, so fifty surfaces referencing one
    /// texture upload once; the caller only has to ask for the same
    /// [`ResolvedTexture`] under the same semantic and profile.
    #[must_use]
    pub fn get_or_upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolved: &ResolvedTexture,
        semantic: TextureSemantic,
        profile: QualityProfile,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        let key = TextureKey::new(resolved, semantic, profile);
        self.get_or_upload_key(device, queue, key, resolved.image.as_ref(), resolved.origin)
    }

    /// Uploads one fitted, clamped single-use sheet under a logical key.
    ///
    /// The prop and fixture sheets are not part of the level's material table:
    /// a model's own PNG and a fixture family's face arrive as decoded
    /// [`RawImage`]s with their own session key. Their GPU identity is the same
    /// kind of cache entry, clamped because fitted UVs never repeat.
    #[must_use]
    #[allow(clippy::too_many_arguments)] // one upload entry point: device/queue plus the fitted sheet's identity
    pub fn get_or_upload_fitted(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        logical: &str,
        image: &RawImage,
        class: TextureClass,
        origin: TextureOrigin,
        profile: QualityProfile,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        let key = TextureKey {
            logical: logical.to_string(),
            semantic: TextureSemantic::BaseColorDisplay,
            class,
            profile,
            wrap: TextureWrap::Clamp,
        };
        self.get_or_upload_key(device, queue, key, image, origin)
    }

    /// The shared body of both upload entry points.
    ///
    /// The key carries the class and profile the fit needs, so no caller can
    /// pass values that disagree with the cache identity.
    fn get_or_upload_key(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: TextureKey,
        image: &RawImage,
        origin: TextureOrigin,
    ) -> (CacheOutcome, Arc<GpuTexture>) {
        if let Some(texture) = self.get(&key, origin).map(Arc::clone) {
            return (CacheOutcome::Reused, texture);
        }
        let fitted = fit_image(image, key.profile, key.class);
        let image: &RawImage = &fitted;
        let texture = Arc::new(GpuTexture::upload(
            device,
            queue,
            &self.layout,
            &self.samplers,
            &TextureUpload {
                key: &key,
                image,
                mip_levels: mip_level_count(image.width, image.height),
                origin,
                fallback: false,
            },
        ));
        self.map_mut(origin).insert(key, Arc::clone(&texture));
        (CacheOutcome::Uploaded, texture)
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

/// The decoded fallback sheet: the committed 2x2 white PNG, or a generated
/// 2x2 white fill if the embedded bytes were ever corrupt (they are pinned by
/// a test, so the second arm is unreachable in practice and exists only so a
/// broken install degrades to white instead of failing to start).
#[must_use]
pub fn fallback_white_image() -> RawImage {
    decode_png(FALLBACK_WHITE_PNG).unwrap_or_else(|_| {
        // Generated fallback of last resort: 2x2 opaque white (16 bytes).
        let mut rgba = vec![0u8; FALLBACK_BYTES];
        rgba.fill(u8::MAX);
        RawImage::new(2, 2, rgba)
    })
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
/// Deterministic and display-space: it averages the raw 8-bit channels without
/// any gamma conversion, which is exactly what the OpenGL reference's
/// `glGenerateMipmap` does to a non-sRGB `GL_RGBA` texture. Every output texel
/// is the average of its 2x2 source block (fewer texels at an odd edge), so a
/// power-of-two source produces the exact successive averages the reference
/// generates. Alpha is averaged the same way and never discarded.
#[must_use]
fn halve_image(source: &RawImage) -> RawImage {
    let width = (source.width / 2).max(1);
    let height = (source.height / 2).max(1);
    let Some(length) = buffer_len(width, height) else {
        return RawImage::new(1, 1, vec![255, 255, 255, 255]);
    };
    let mut rgba = vec![0u8; length];
    for out_y in 0..height {
        let y0 = out_y.saturating_mul(2);
        let y1 = y0.saturating_add(2).min(source.height.max(1));
        for out_x in 0..width {
            let x0 = out_x.saturating_mul(2);
            let x1 = x0.saturating_add(2).min(source.width.max(1));
            let mut sums = [0u32; 4];
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let Some(texel) = texel(source, x, y) else {
                        continue;
                    };
                    for (sum, channel) in sums.iter_mut().zip(texel) {
                        *sum = sum.saturating_add(u32::from(channel));
                    }
                    count = count.saturating_add(1);
                }
            }
            if count == 0 {
                continue;
            }
            let Some(offset) = texel_offset(out_x, out_y, width) else {
                continue;
            };
            for (index, sum) in sums.iter().enumerate() {
                let rounded = sum
                    .saturating_add(count / 2)
                    .checked_div(count)
                    .unwrap_or(0)
                    .min(u32::from(u8::MAX));
                let value = u8::try_from(rounded).unwrap_or(u8::MAX);
                if let Some(slot) = rgba.get_mut(offset.saturating_add(index)) {
                    *slot = value;
                }
            }
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

/// Byte offset of texel `(x, y)` in a tightly packed RGBA8 buffer.
fn texel_offset(x: u32, y: u32, width: u32) -> Option<usize> {
    let row = usize::try_from(y)
        .ok()?
        .checked_mul(usize::try_from(width).ok()?)?
        .checked_mul(usize::try_from(RGBA_BYTES).ok()?)?;
    let column = usize::try_from(x)
        .ok()?
        .checked_mul(usize::try_from(RGBA_BYTES).ok()?)?;
    row.checked_add(column)
}

/// The four channels of one texel, or `None` outside the image.
fn texel(source: &RawImage, x: u32, y: u32) -> Option<[u8; 4]> {
    let offset = texel_offset(x, y, source.width)?;
    let end = offset.checked_add(usize::try_from(RGBA_BYTES).ok()?)?;
    let slice = source.rgba.get(offset..end)?;
    <[u8; 4]>::try_from(slice).ok()
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

#[cfg(test)]
mod tests {
    // Test code: unwrap/indexing/float comparisons are idiomatic here.
    #![allow(
        clippy::arithmetic_side_effects,
        clippy::expect_used,
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used
    )]

    use super::*;

    /// A raw RGBA8 image over the given dimensions and texel function.
    fn image(width: u32, height: u32, texel: impl Fn(u32, u32) -> [u8; 4]) -> RawImage {
        let mut rgba = Vec::with_capacity((width * height) as usize * 4);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&texel(x, y));
            }
        }
        RawImage::new(width, height, rgba)
    }

    /// One channel value of one texel.
    fn channel(image: &RawImage, x: u32, y: u32, channel: usize) -> u8 {
        let offset = ((y * image.width + x) * 4) as usize + channel;
        image.rgba[offset]
    }

    // ------------------------------------------------------ colour semantics

    #[test]
    fn every_semantic_samples_raw_display_space() {
        // Stage 10: the reference has no sRGB textures, so neither does the
        // port. The one display-to-surface conversion is the shader's
        // `srgb_to_linear` at the final sRGB surface.
        assert_eq!(
            TextureSemantic::BaseColorDisplay.format(),
            wgpu::TextureFormat::Rgba8Unorm
        );
        assert_eq!(
            TextureSemantic::DataLinear.format(),
            wgpu::TextureFormat::Rgba8Unorm
        );
        assert!(
            !TextureSemantic::BaseColorDisplay.format().is_srgb(),
            "authored colour is sampled as display bytes, exactly like the reference"
        );
        assert!(
            !TextureSemantic::DataLinear.format().is_srgb(),
            "a normal map or mask must never receive an sRGB decode"
        );
    }

    #[test]
    fn the_texture_key_separates_semantic_class_and_profile() {
        let resolved = ResolvedTexture {
            key: "core:tex_thing_01".to_string(),
            origin: TextureOrigin::Catalog,
            class: TextureClass::Surface,
            image: std::rc::Rc::new(image(1, 1, |_, _| [1, 2, 3, 4])),
        };
        let base = TextureKey::new(
            &resolved,
            TextureSemantic::BaseColorDisplay,
            QualityProfile::Full,
        );
        assert_eq!(base.logical, "core:tex_thing_01");
        assert_ne!(
            base,
            TextureKey::new(&resolved, TextureSemantic::DataLinear, QualityProfile::Full)
        );
        assert_ne!(
            base,
            TextureKey::new(
                &resolved,
                TextureSemantic::BaseColorDisplay,
                QualityProfile::Low
            )
        );
        let other_class = ResolvedTexture {
            class: TextureClass::EmissionMask,
            ..resolved
        };
        assert_ne!(
            base,
            TextureKey::new(
                &other_class,
                TextureSemantic::BaseColorDisplay,
                QualityProfile::Full
            )
        );
    }

    // -------------------------------------------------------------- mip math

    #[test]
    fn the_mip_count_is_the_full_chain_down_to_one_texel() {
        assert_eq!(mip_level_count(1, 1), 1);
        assert_eq!(mip_level_count(2, 2), 2);
        assert_eq!(mip_level_count(4, 4), 3);
        assert_eq!(mip_level_count(1024, 1024), 11);
        assert_eq!(
            mip_level_count(1024, 1),
            11,
            "non-square keeps the long chain"
        );
        assert_eq!(mip_level_count(96, 64), 7);
        assert_eq!(mip_level_count(3, 3), 2, "odd edges floor");
        assert_eq!(mip_level_count(0, 0), 1, "zero is never a real image");
    }

    #[test]
    fn the_mip_chain_stops_at_one_by_one_and_halves_every_level() {
        let mut current = image(96, 64, |x, y| {
            [
                u8::try_from(x % 251).unwrap_or(0),
                u8::try_from(y % 251).unwrap_or(0),
                7,
                255,
            ]
        });
        let levels = mip_level_count(current.width, current.height);
        let mut seen = vec![(current.width, current.height)];
        for _ in 1..levels {
            current = halve_image(&current);
            seen.push((current.width, current.height));
        }
        assert_eq!(
            seen,
            vec![
                (96, 64),
                (48, 32),
                (24, 16),
                (12, 8),
                (6, 4),
                (3, 2),
                (1, 1)
            ]
        );
    }

    #[test]
    fn halving_averages_each_two_by_two_block() {
        // A 4x4 image with one channel per texel position, alpha constant.
        let source = image(4, 4, |x, y| {
            [u8::try_from(x * 4 + y).unwrap_or(0), 100, 0, 255]
        });
        let half = halve_image(&source);
        assert_eq!((half.width, half.height), (2, 2));
        assert_eq!(half.rgba.len(), 2 * 2 * 4);
        // Block (0,0): channels 0,1 / 4,5 -> mean of the first channel 2.5 -> 3.
        assert_eq!(channel(&half, 0, 0, 0), 3);
        assert_eq!(channel(&half, 0, 0, 1), 100);
        assert_eq!(channel(&half, 0, 0, 3), 255);
        // Block (1,1): channels 10,11 / 14,15 -> mean 12.5 -> 13.
        assert_eq!(channel(&half, 1, 1, 0), 13);
    }

    #[test]
    fn halving_an_odd_image_clamps_the_edge_block() {
        // 3x3 of constant 8: the last row/column blocks hold fewer texels, but
        // a constant image must stay constant.
        let source = image(3, 3, |_, _| [8, 8, 8, 9]);
        let half = halve_image(&source);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.rgba, vec![8, 8, 8, 9]);
    }

    #[test]
    fn halving_one_by_one_is_idempotent() {
        let source = image(1, 1, |_, _| [200, 100, 50, 25]);
        let half = halve_image(&source);
        assert_eq!((half.width, half.height), (1, 1));
        assert_eq!(half.rgba, source.rgba);
    }

    #[test]
    fn a_constant_image_has_a_constant_chain() {
        let mut current = image(5, 3, |_, _| [42, 43, 44, 45]);
        for _ in 1..mip_level_count(5, 3) {
            current = halve_image(&current);
            assert!(
                current
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|t| *t == [42, 43, 44, 45]),
                "a constant source must not drift: {:?} at {}x{}",
                current.rgba,
                current.width,
                current.height
            );
        }
    }

    // ---------------------------------------------------------- upload math

    #[test]
    fn rows_are_tightly_packed_and_never_padded() {
        assert_eq!(row_bytes(1), 4);
        assert_eq!(row_bytes(96), 384, "96x64 is not 256-byte aligned");
        assert_eq!(row_bytes(1024), 4096);
        assert_eq!(row_bytes(0), 0);
    }

    #[test]
    fn the_resident_byte_count_covers_every_level() {
        let bytes = |w: u32, h: u32| {
            let mut total = level_bytes(w, h);
            let mut current_width = w;
            let mut current_height = h;
            while current_width > 1 || current_height > 1 {
                current_width = (current_width / 2).max(1);
                current_height = (current_height / 2).max(1);
                total += level_bytes(current_width, current_height);
            }
            total
        };
        assert_eq!(bytes(2, 2), 16 + 4);
        assert_eq!(bytes(4, 4), 64 + 16 + 4);
        assert_eq!(bytes(1, 1), 4);
        assert_eq!(
            bytes(1024, 1024),
            4 * (1024 * 1024
                + 512 * 512
                + 256 * 256
                + 128 * 128
                + 64 * 64
                + 32 * 32
                + 16 * 16
                + 8 * 8
                + 4 * 4
                + 2 * 2
                + 1)
        );
    }

    // -------------------------------------------------------------- fallback

    #[test]
    fn the_fallback_is_the_committed_two_by_two_white_sheet() {
        let image = fallback_white_image();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            image.rgba,
            vec![255; 16],
            "the fallback must stay opaque white"
        );
    }

    #[test]
    fn the_embedded_fallback_is_the_committed_asset() {
        // The same bytes the preserved reference renderer embedded
        // (`WHITE_SHEET_PNG` in its OpenGL renderer module).
        assert!(
            FALLBACK_WHITE_PNG.starts_with(b"\x89PNG\r\n\x1a\n"),
            "the fallback must be the committed PNG, not a generated fill"
        );
        assert_eq!(
            decode_png(FALLBACK_WHITE_PNG).expect("the committed sheet decodes"),
            fallback_white_image()
        );
    }

    // -------------------------------------------------------------- samplers

    #[test]
    fn the_repeating_policies_repeat_and_the_fallback_policy_clamps() {
        for policy in [SamplerPolicy::RepeatLinear, SamplerPolicy::RepeatNearest] {
            let descriptor = policy.descriptor();
            assert_eq!(descriptor.address_mode_u, wgpu::AddressMode::Repeat);
            assert_eq!(descriptor.address_mode_v, wgpu::AddressMode::Repeat);
            assert_eq!(descriptor.address_mode_w, wgpu::AddressMode::Repeat);
        }
        let clamp = SamplerPolicy::ClampNearest.descriptor();
        assert_eq!(clamp.address_mode_u, wgpu::AddressMode::ClampToEdge);
        assert_eq!(clamp.address_mode_v, wgpu::AddressMode::ClampToEdge);
        assert_eq!(clamp.address_mode_w, wgpu::AddressMode::ClampToEdge);
    }

    #[test]
    fn the_filtering_setting_selects_matching_sampler_filters() {
        let linear = SamplerPolicy::RepeatLinear.descriptor();
        assert_eq!(linear.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(linear.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(linear.mipmap_filter, wgpu::MipmapFilterMode::Linear);

        let nearest = SamplerPolicy::RepeatNearest.descriptor();
        assert_eq!(nearest.mag_filter, wgpu::FilterMode::Nearest);
        assert_eq!(nearest.min_filter, wgpu::FilterMode::Nearest);
        assert_eq!(nearest.mipmap_filter, wgpu::MipmapFilterMode::Nearest);

        // No policy uses a comparison sampler or anisotropy; the reference has
        // neither, and base colour needs neither.
        for policy in [
            SamplerPolicy::RepeatLinear,
            SamplerPolicy::RepeatNearest,
            SamplerPolicy::ClampNearest,
        ] {
            let descriptor = policy.descriptor();
            assert!(descriptor.compare.is_none());
            assert_eq!(descriptor.anisotropy_clamp, 1);
            assert_eq!(descriptor.lod_min_clamp, 0.0);
            assert_eq!(descriptor.lod_max_clamp, 32.0);
        }
    }

    #[test]
    fn the_filtering_setting_parses_like_the_reference() {
        assert_eq!(TextureFiltering::parse("linear"), TextureFiltering::Linear);
        assert_eq!(
            TextureFiltering::parse("nearest"),
            TextureFiltering::Nearest
        );
        assert_eq!(TextureFiltering::parse(""), TextureFiltering::Linear);
        assert_eq!(TextureFiltering::parse("bogus"), TextureFiltering::Linear);
        assert_eq!(TextureFiltering::default(), TextureFiltering::Linear);
        assert_eq!(
            TextureFiltering::Linear.sampler_policy(),
            SamplerPolicy::RepeatLinear
        );
        assert_eq!(
            TextureFiltering::Nearest.sampler_policy(),
            SamplerPolicy::RepeatNearest
        );
    }

    #[test]
    fn the_clamped_sheet_policies_follow_the_user_setting() {
        // A fitted single-use sheet (prop, fixture, emissive mask) clamps in
        // both modes, like the reference's `upload_fitted_texture`.
        assert_eq!(
            TextureFiltering::Linear.clamp_sampler_policy(),
            SamplerPolicy::ClampLinear
        );
        assert_eq!(
            TextureFiltering::Nearest.clamp_sampler_policy(),
            SamplerPolicy::ClampNearest
        );
        let linear = SamplerPolicy::ClampLinear.descriptor();
        assert_eq!(linear.address_mode_u, wgpu::AddressMode::ClampToEdge);
        assert_eq!(linear.address_mode_v, wgpu::AddressMode::ClampToEdge);
        assert_eq!(linear.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(linear.min_filter, wgpu::FilterMode::Linear);
        assert_eq!(linear.mipmap_filter, wgpu::MipmapFilterMode::Linear);
    }

    #[test]
    fn a_wrap_selects_the_matching_sampler_policy() {
        assert_eq!(
            TextureWrap::Repeat.policy(TextureFiltering::Linear),
            SamplerPolicy::RepeatLinear
        );
        assert_eq!(
            TextureWrap::Repeat.policy(TextureFiltering::Nearest),
            SamplerPolicy::RepeatNearest
        );
        assert_eq!(
            TextureWrap::Clamp.policy(TextureFiltering::Linear),
            SamplerPolicy::ClampLinear
        );
        assert_eq!(
            TextureWrap::Clamp.policy(TextureFiltering::Nearest),
            SamplerPolicy::ClampNearest
        );
    }

    /// Measures the hardware sRGB decode plus the shader-style `linear_to_srgb`
    /// encode on this adapter, byte by byte.
    ///
    /// Stage 10 measured this after Stage 9's canonical replay left a broad
    /// +1 residue on minified surfaces. The result (0 error for all 256 bytes
    /// on Apple/Metal) ruled the per-texel round trip out as the cause and
    /// pointed at *blending* decoded values: the pipeline now samples raw
    /// `Rgba8Unorm`, and this test stays as the adapter-level contract that
    /// made the repair a measurement rather than a guess.
    ///
    /// Ignored by default: it needs a real adapter. Run with:
    ///
    /// ```text
    /// cargo test --all-features --bin places -- --ignored the_srgb_sample_round_trip
    /// ```
    #[test]
    #[ignore = "requires a GPU adapter"]
    #[allow(clippy::too_many_lines)] // one self-contained GPU measurement: setup, shader, dispatch, compare
    fn the_srgb_sample_round_trip_is_measured_on_this_adapter() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .expect("a GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("places-srgb-round-trip"),
            ..Default::default()
        }))
        .expect("a device");

        // One texel per byte value: `(b, b, b, 255)` at x = b.
        let pixels: Vec<u8> = (0..=255u8).flat_map(|b| [b, b, b, 255u8]).collect();
        let source = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("srgb-round-trip-source"),
            size: wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let source_view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let results = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("srgb-round-trip-results"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("srgb-round-trip-readback"),
            size: 256 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("srgb-round-trip"),
            source: wgpu::ShaderSource::Wgsl(
                r"
fn linear_to_srgb(c: f32) -> f32 {
    let low = c * 12.92;
    let high = 1.055 * pow(c, 1.0 / 2.4) - 0.055;
    return select(high, low, c <= 0.0031308);
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> out_bytes: array<u32, 256>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= 256u) { return; }
    let sample = textureLoad(source, vec2<i32>(i32(id.x), 0), 0);
    let display = linear_to_srgb(sample.r);
    out_bytes[id.x] = u32(round(clamp(display, 0.0, 1.0) * 255.0));
}
"
                .into(),
            ),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("srgb-round-trip-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("srgb-round-trip-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("srgb-round-trip"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("srgb-round-trip"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: results.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("srgb-round-trip"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("srgb-round-trip"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&results, 0, &readback, 0, 256 * 4);
        queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        receiver.recv().expect("map callback").expect("buffer maps");
        let data = slice.get_mapped_range().expect("mapped range");
        let mut errors = std::collections::BTreeMap::<i32, usize>::new();
        let mut worst = 0i32;
        for (value, word) in (0..256i32).zip(data.as_chunks::<4>().0.iter()) {
            let byte = i32::from_le_bytes(*word);
            let error = byte - value;
            *errors.entry(error).or_default() += 1;
            worst = worst.max(error.abs());
        }
        drop(data);
        readback.unmap();
        // The measurement is the test's purpose: report the full distribution
        // so a backend whose hardware decode differs is visible in the log.
        // Printing is deliberate here, so the crate's stdout lint is allowed.
        #[allow(clippy::print_stdout)]
        {
            println!("srgb round-trip error distribution: {errors:?}");
        }
        assert!(
            worst <= 1,
            "the hardware sRGB decode must invert the IEC encode within one byte: {errors:?}"
        );
    }
}

//! Full and Low runtime quality profiles.
//!
//! Places deliberately has exactly two quality modes — there are no
//! hardware-specific tiers, no auto-detection and no per-setting zoo. A profile
//! answers one question: **how much texture data may reach the GPU?**
//!
//! ```text
//! source PNG (asset)  ──decode──▶  Full runtime image  ──upload──▶  GPU
//!                     ──decode──▶  Low runtime image (more aggressively scaled)
//! ```
//!
//! Both profiles use the same levels and the same source assets. Low is not a
//! second art library: it is the same PNG, downscaled further, so the visual
//! identity (and every id, material and fixture) stays identical.
//!
//! Full uploads the native artwork unchanged — a 256x256 prop atlas stays
//! 256x256, and a 1024x1024 surface sheet stays 1024x1024. Low is an optional
//! display-budget reduction for players who want it: sheets at 256 and prop
//! sheets at 128, which is a 16x reduction in texel count for a sheet and a 4x
//! reduction for a prop. It is an intentional quality/performance trade, not a
//! hardware requirement of the desktop target, and it never changes the size
//! of the asset stored in the repository.
//!
//! Texture discipline is unchanged: the decoder still refuses anything above
//! [`crate::assets::MAX_TEXTURE_DIMENSION`], and the shipped prop art budget
//! still caps a prop atlas at its 256x256 native size. A quality profile only
//! decides how much of an *accepted* source reaches the GPU; it never raises
//! the source limit.
//!
//! The same profile also budgets the static lightmap atlas: `Full` bakes at 12
//! texels per metre onto up to two 1024-texel pages, `Low` at 8 texels per
//! metre onto two 512-texel pages (see
//! [`crate::lighting::lightmap::LightmapConfig::for_profile`]). Both profiles
//! bake from the *same* patch set — the density and page size are the only
//! difference, never a different set of surfaces — and both target the same
//! world span per chart, so the geometry splits in the same places.
//!
//! Downscaling happens once, at upload/level-load time, through
//! [`crate::materials::RawImage::downscaled_to`] and is cached with the texture
//! it produced — never per frame, and never twice for the same image. A
//! lightmap atlas is likewise baked once per level load and cached by content
//! key; nothing here runs per frame.

/// One of Places' two runtime quality profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QualityProfile {
    /// The intended normal Places presentation: native textures, unchanged.
    #[default]
    Full,
    /// The optional reduced-texture presentation: same assets, smaller sheets.
    Low,
}

/// The texture classes a quality profile budgets separately.
///
/// They are separate because their authored sizes and their sampling duties
/// differ: a tiling surface sheet covers metres of wall, a fitted fixture face
/// covers one panel, and a prop sheet covers one model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureClass {
    /// A tiling surface sheet (wall, floor, ceiling).
    Surface,
    /// A fitted fixture face.
    FixtureFace,
    /// A decal cut-out sheet.
    DecalSheet,
    /// A prop model's embedded sheet.
    Prop,
    /// A material's emissive mask.
    EmissionMask,
}

/// Full-quality edge budget: surfaces and fitted sheets at 1024.
const FULL_SHEET_EDGE: u32 = 1_024;
/// Full-quality edge budget: props at their native 256, uploaded unchanged.
const FULL_PROP_EDGE: u32 = 256;
/// Full-quality edge budget: emissive masks at 512.
const FULL_MASK_EDGE: u32 = 512;
/// Low-quality edge budget: sheets at 256.
const LOW_SHEET_EDGE: u32 = 256;
/// Low-quality edge budget: native props halved once, 256 -> 128.
const LOW_PROP_EDGE: u32 = 128;
/// Low-quality edge budget: emissive masks at 128.
const LOW_MASK_EDGE: u32 = 128;

impl QualityProfile {
    /// Every profile, in report order.
    pub const ALL: [Self; 2] = [Self::Full, Self::Low];

    /// The profile a level loads with when nothing is authored.
    pub const DEFAULT: Self = Self::Full;

    /// Stable lowercase name, as written in `settings.json` and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Low => "low",
        }
    }

    /// Parses a profile name, case-insensitively and ignoring surrounding
    /// whitespace. Unknown names are `None` (the caller keeps its current
    /// profile), never a silent fallback.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|profile| profile.name().eq_ignore_ascii_case(trimmed))
    }

    /// Largest edge length, in texels, an image of `class` may reach the GPU
    /// with under this profile.
    ///
    /// The returned value is a *runtime* budget: the source PNG may legally be
    /// larger (up to [`crate::assets::MAX_TEXTURE_DIMENSION`]) and is downscaled
    /// once, at load time, to fit.
    #[must_use]
    pub const fn budget(self, class: TextureClass) -> u32 {
        match (self, class) {
            (
                Self::Full,
                TextureClass::Surface | TextureClass::FixtureFace | TextureClass::DecalSheet,
            ) => FULL_SHEET_EDGE,
            (Self::Full, TextureClass::Prop) => FULL_PROP_EDGE,
            (Self::Full, TextureClass::EmissionMask) => FULL_MASK_EDGE,
            (
                Self::Low,
                TextureClass::Surface | TextureClass::FixtureFace | TextureClass::DecalSheet,
            ) => LOW_SHEET_EDGE,
            (Self::Low, TextureClass::Prop) => LOW_PROP_EDGE,
            (Self::Low, TextureClass::EmissionMask) => LOW_MASK_EDGE,
        }
    }

    /// True when this profile drops optional per-pixel material work.
    ///
    /// Reserving the hook now keeps later effect work (a future reflection or
    /// post-processing path) from having to invent its own tier names: it asks
    /// the active profile instead.
    #[must_use]
    pub const fn reduces_optional_features(self) -> bool {
        matches!(self, Self::Low)
    }

    /// True when this profile draws the optional surface response.
    ///
    /// The response is the normal-map perturbation and the view-dependent sheen
    /// a material may author ([`crate::materials::response`]). Both profiles draw
    /// the same geometry and the same albedo, emission and alpha; Low simply
    /// leaves the response term out, which is the one per-fragment cost a
    /// constrained GPU can drop without changing what an author authored. It is
    /// a shader gate, not a different asset: the same PNGs and the same
    /// materials reach the GPU under either profile.
    #[must_use]
    pub const fn draws_surface_response(self) -> bool {
        matches!(self, Self::Full)
    }

    /// True when this profile renders the 3D scene at the drawable's own
    /// resolution.
    ///
    /// See [`crate::render::framebuffer`]: Low renders the scene no wider than
    /// the historical 480x272 reference width and presents it across the
    /// drawable, trading scene pixels for performance in the optional Low
    /// presentation.
    #[must_use]
    pub const fn draws_scene_at_drawable_resolution(self) -> bool {
        matches!(self, Self::Full)
    }
}

#[cfg(test)]
mod tests;

/// Returns the image a texture uploads with under `profile`.
///
/// Full keeps the decoded image exactly as it is — native 256x256 prop sheets
/// and 1024x1024 surfaces are uploaded unchanged, with no rescale and no copy.
/// Low box-filters it once. The caller does this at upload time and keeps the
/// result with the texture it uploaded, so an image is never rescaled per
/// frame, nor twice for one upload.
#[must_use]
pub fn fit_image(
    image: &crate::materials::RawImage,
    profile: QualityProfile,
    class: TextureClass,
) -> std::borrow::Cow<'_, crate::materials::RawImage> {
    image.downscaled_to(profile.budget(class)).map_or_else(
        || std::borrow::Cow::Borrowed(image),
        std::borrow::Cow::Owned,
    )
}

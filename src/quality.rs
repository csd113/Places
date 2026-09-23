//! Full and Low runtime quality profiles.
//!
//! Places deliberately has exactly two quality modes — there are no
//! hardware-specific tiers, no auto-detection and no per-setting zoo. A profile
//! answers one question: **how large may a texture be once it reaches the GPU?**
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
//! Full keeps the sizes the project has always used at runtime — surfaces and
//! fitted sheets at up to 1024, prop sheets at up to 256 — so existing content
//! renders exactly as it did. Low applies the PocketCHIP/Mali-400 budget from
//! `assets/README.md`: sheets at 256 and prop sheets at 128, which is a 16x
//! reduction in texel count for a sheet and a 4x reduction for a prop.
//!
//! Texture discipline is unchanged: the decoder still refuses anything above
//! [`crate::assets::MAX_TEXTURE_DIMENSION`], and art-budget guidance still
//! prefers far smaller sheets. A quality profile only decides how much of an
//! *accepted* source reaches the GPU; it never raises the source limit.
//!
//! Downscaling happens once, at upload/level-load time, through
//! [`crate::materials::RawImage::downscaled_to`] and is cached with the texture
//! it produced — never per frame, and never twice for the same image.

/// One of Places' two runtime quality profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QualityProfile {
    /// The intended normal Places presentation.
    #[default]
    Full,
    /// The constrained-hardware presentation: same assets, smaller textures.
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
/// Full-quality edge budget: props at 256.
const FULL_PROP_EDGE: u32 = 256;
/// Full-quality edge budget: emissive masks at 512.
const FULL_MASK_EDGE: u32 = 512;
/// Low-quality edge budget: sheets at 256.
const LOW_SHEET_EDGE: u32 = 256;
/// Low-quality edge budget: props at 128.
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
    /// the active profile instead. Nothing in this batch is switched off yet.
    #[must_use]
    pub const fn reduces_optional_features(self) -> bool {
        matches!(self, Self::Low)
    }
}

#[cfg(test)]
mod tests;

/// Returns the image a texture uploads with under `profile`.
///
/// Full keeps the decoded image exactly as it is — every shipped asset is at or
/// below the historical runtime size, so nothing is rescaled and no copy is
/// made. Low box-filters it once. The caller does this at upload time and keeps
/// the result with the texture it uploaded, so an image is never rescaled per
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

//! Lightweight surface response: normal, specular and roughness.
//!
//! Places deliberately does **not** have a physically based material model.
//! This module is the smallest set of numbers that makes two surfaces read
//! differently under the *existing* baked illumination:
//!
//! ```text
//! dull painted wall   specular 0.0                 roughness 1.0
//! plastic             specular 0.35 (white)        roughness 0.35
//! brushed metal       specular 0.55 (own colour)   roughness 0.25
//! glossy tile         specular 0.45 (white)        roughness 0.15
//! polished floor      specular 0.60 (white)        roughness 0.10
//! wet surface         specular 0.70 (white)        roughness 0.05
//! ```
//!
//! The response is *added on top of* the baked light, exactly like emission:
//! it never replaces the lighting model, never introduces a realtime light and
//! never needs a shadow map. A surface in a dark room stays dark; a glossy
//! surface merely catches more of whatever light is already there.
//!
//! ```text
//! what a surface draws  = texture x tint x baked light
//!                       + emission (material's own brightness)
//!                       + response (view-dependent sheen, scaled by the baked light)
//! ```
//!
//! Three properties, each optional and each with a default that reproduces the
//! legacy flat-shaded look exactly:
//!
//! * **Normal map** ([`MaterialResponse::normal`]) — an optional texture whose
//!   texels perturb the shading normal, plus a strength multiplier. Absent (the
//!   default) means the geometric normal alone, which is what every material
//!   authored before this existed keeps.
//! * **Specular** ([`MaterialResponse::specular`]) — the colour of the sheen,
//!   normally a scalar strength multiplied by white, optionally tinted (a metal
//!   catches its own colour). Zero (the default) adds nothing at all.
//! * **Roughness** ([`MaterialResponse::roughness`]) — how tightly the sheen
//!   concentrates towards grazing angles. One (the default) is fully matte and
//!   produces no visible term even if a specular strength is authored.
//!
//! The response is *view dependent*: it brightens where the surface turns away
//! from the camera, which is what makes a polished floor catch the room's light
//! and a painted wall stay flat. It is not a reflection and it does not sample
//! the framebuffer.
//!
//! Alpha
//! -----
//! [`MaterialAlpha`] lives beside the response because it is the same kind of
//! contract — how a *material* draws, not how a texture is stored:
//!
//! * [`AlphaMode::Opaque`] (the default) ignores the texture's alpha channel
//!   entirely and writes opaque pixels, which is every material authored before
//!   transparency existed.
//! * [`AlphaMode::Cutout`] discards texels below
//!   [`MaterialAlpha::cutoff`] and writes the rest opaquely.
//! * [`AlphaMode::Blend`] draws the surface in the sorted translucent pass with
//!   the texture's alpha (scaled by [`MaterialAlpha::opacity`]) as its coverage.
//!
//! Alpha is part of the material, never a render order flag: a level says
//! *what a surface is*, and the renderer decides which pass it lands in.

/// Largest accepted normal-map strength multiplier.
pub const MAX_NORMAL_STRENGTH: f32 = 2.0;

/// Normal-map strength used when a material authors a normal map but no
/// explicit strength.
pub const DEFAULT_NORMAL_STRENGTH: f32 = 1.0;

/// Largest accepted specular strength (and specular colour channel).
pub const MAX_SPECULAR: f32 = 1.0;

/// Roughness a fully matte surface authors.
///
/// `roughness` is a `0.0..=1.0` scale where one is completely matte, so this is
/// the upper bound [`MaterialResponse::sanitized`] clamps to.
pub const MAX_ROUGHNESS: f32 = 1.0;

/// Roughness a material keeps when it authors none.
///
/// It only matters to a material that also authors a specular strength — a
/// material without one has no sheen to shape — so the default is the visibly
/// glossy-but-not-mirror middle: `specular: 0.4` on its own reads as plastic,
/// and an author wanting a matte surface writes `roughness: 1.0`.
pub const DEFAULT_ROUGHNESS: f32 = 0.6;

/// Default alpha cut-off of a [`AlphaMode::Cutout`] material.
pub const DEFAULT_ALPHA_CUTOFF: f32 = 0.5;

/// The response half of a material: how its surface reacts to the light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialResponse {
    /// Optional normal-map texture, as an index into the owner's texture list
    /// (the material table for level materials). `None` means the geometric
    /// normal alone.
    pub normal: Option<u16>,
    /// Multiplier applied to the normal map's decoded `xy`, `0.0..=2.0`.
    pub normal_strength: f32,
    /// Sheen colour: a scalar strength premultiplied by white, or an authored
    /// RGB. Each channel is `0.0..=1.0`; zero means no sheen at all.
    pub specular: [f32; 3],
    /// `0.0` is a mirror-tight sheen, `1.0` is fully matte.
    pub roughness: f32,
}

impl MaterialResponse {
    /// No response: a material that authors no normal map and no sheen.
    pub const NONE: Self = Self {
        normal: None,
        normal_strength: DEFAULT_NORMAL_STRENGTH,
        specular: [0.0; 3],
        roughness: DEFAULT_ROUGHNESS,
    };

    /// A response with a normal map and no sheen.
    #[must_use]
    pub const fn with_normal(normal: Option<u16>, normal_strength: f32) -> Self {
        Self {
            normal,
            normal_strength,
            ..Self::NONE
        }
    }

    /// A sheen of the given strength, in white, at the given roughness.
    #[must_use]
    pub const fn with_sheen(strength: f32, roughness: f32) -> Self {
        Self {
            specular: [strength; 3],
            roughness,
            ..Self::NONE
        }
    }

    /// True when this response can change a pixel at all.
    ///
    /// A response without a normal map and without a sheen is exactly
    /// [`MaterialResponse::NONE`], so the draw path can skip its uniform work
    /// (and its normal-map texture fetch) for every legacy material.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.has_normal() || self.has_sheen()
    }

    /// True when a normal map is bound.
    #[must_use]
    pub const fn has_normal(&self) -> bool {
        self.normal.is_some()
    }

    /// True when the material has a sheen that can actually show.
    ///
    /// Roughness only shapes the sheen; the strength is what switches it on, so
    /// a material without a specular colour has no sheen at any roughness.
    #[must_use]
    pub fn has_sheen(&self) -> bool {
        self.specular[0] > 0.0 || self.specular[1] > 0.0 || self.specular[2] > 0.0
    }

    /// Clamps every value into its documented range, mapping non-finite input
    /// to the neutral default rather than to a NaN a shader would spread.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let channel = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, MAX_SPECULAR)
            } else {
                0.0
            }
        };
        let strength = if self.normal_strength.is_finite() {
            self.normal_strength.clamp(0.0, MAX_NORMAL_STRENGTH)
        } else {
            DEFAULT_NORMAL_STRENGTH
        };
        let roughness = if self.roughness.is_finite() {
            self.roughness.clamp(0.0, MAX_ROUGHNESS)
        } else {
            DEFAULT_ROUGHNESS
        };
        Self {
            normal: self.normal,
            normal_strength: strength,
            specular: [
                channel(self.specular[0]),
                channel(self.specular[1]),
                channel(self.specular[2]),
            ],
            roughness,
        }
    }
}

impl Default for MaterialResponse {
    fn default() -> Self {
        Self::NONE
    }
}

/// How a material's pixels combine with what is already in the framebuffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    /// Ignore the texture's alpha channel; every texel is opaque. The default,
    /// and what every material authored before transparency existed gets.
    #[default]
    Opaque,
    /// Discard texels below the material's cut-off, write the rest opaquely.
    /// Drawn in the opaque pass with an alpha-tested fragment stage.
    Cutout,
    /// Blend the surface over the framebuffer with its alpha as coverage.
    /// Drawn in the sorted, depth-write-disabled translucent pass.
    Blend,
}

impl AlphaMode {
    /// Every mode, in report order.
    pub const ALL: [Self; 3] = [Self::Opaque, Self::Cutout, Self::Blend];

    /// Stable lowercase name, as written in the catalog and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Cutout => "cutout",
            Self::Blend => "blend",
        }
    }

    /// Parses a mode name, case-insensitively. Unknown names are `None`, which
    /// the caller reports as a catalog error rather than silently ignoring.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(trimmed))
    }

    /// True when the surface must be drawn in the translucent pass.
    #[must_use]
    pub const fn is_translucent(self) -> bool {
        matches!(self, Self::Blend)
    }

    /// True when the fragment stage must discard texels below the cut-off.
    #[must_use]
    pub const fn is_cutout(self) -> bool {
        matches!(self, Self::Cutout)
    }
}

/// The alpha half of a material: mode, opacity multiplier and cut-off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialAlpha {
    pub mode: AlphaMode,
    /// Multiplier applied to the sampled alpha, `0.0..=1.0`.
    pub opacity: f32,
    /// Alpha below which a [`AlphaMode::Cutout`] texel is discarded.
    pub cutoff: f32,
}

impl MaterialAlpha {
    /// Fully opaque: the state of every material without alpha fields.
    pub const OPAQUE: Self = Self {
        mode: AlphaMode::Opaque,
        opacity: 1.0,
        cutoff: DEFAULT_ALPHA_CUTOFF,
    };

    /// A translucent material at the given opacity.
    #[must_use]
    pub const fn blend(opacity: f32) -> Self {
        Self {
            mode: AlphaMode::Blend,
            opacity,
            cutoff: DEFAULT_ALPHA_CUTOFF,
        }
    }

    /// True when this material draws in the translucent pass.
    #[must_use]
    pub const fn is_translucent(&self) -> bool {
        self.mode.is_translucent() && self.opacity > 0.0
    }

    /// True when this material needs the alpha-tested fragment stage.
    #[must_use]
    pub const fn is_cutout(&self) -> bool {
        self.mode.is_cutout()
    }

    /// Clamps every value into its documented range.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let unit = |value: f32, fallback: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                fallback
            }
        };
        Self {
            mode: self.mode,
            opacity: unit(self.opacity, 1.0),
            cutoff: unit(self.cutoff, DEFAULT_ALPHA_CUTOFF),
        }
    }
}

impl Default for MaterialAlpha {
    fn default() -> Self {
        Self::OPAQUE
    }
}

#[cfg(test)]
mod tests;

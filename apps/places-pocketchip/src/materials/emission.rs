//! True material emission: how bright a surface *reads*.
//!
//! Emission is a material property and nothing else. A surface that emits
//! strongly is drawn bright even when the room's baked illumination is dark,
//! and it does **not** illuminate anything around it. Environmental
//! illumination is always produced by a generic light source
//! ([`crate::lighting::LightSource`]) that some visible object owns; an
//! emissive material never creates, scales or implies one.
//!
//! The two numbers are therefore authored separately and can differ freely:
//!
//! ```text
//!     arcade screen:  emission 2.4 (bright pixels)   light: none
//!     vending machine: emission 0.8 (front panel)    light: subtle, 0.15, blue
//!     EXIT sign:      emission 3.0 (bright face)     light: none
//! ```
//!
//! Representation
//! --------------
//! [`MaterialEmission`] is the whole emission contract: an RGB colour, a scalar
//! intensity and an optional mask texture. The resolved colour
//! ([`MaterialEmission::effective_color`]) is `color * intensity`, which is the
//! value a renderer uploads; the mask selects *where* on the surface emission
//! applies. Emission is combined with baked illumination by the fragment
//! shader as an additive term, so an old material with [`MaterialEmission::NONE`]
//! is arithmetically unchanged.
//!
//! This struct is deliberately the place later material work grows:
//! `albedo`, `normal`, `specular`, `roughness` and `opacity` belong beside it
//! as their own fields, not inside an emission-specific side channel.

/// Largest accepted emissive colour channel.
pub const MAX_EMISSION_COLOR: f32 = 1.0;

/// Largest accepted emissive intensity multiplier.
///
/// Emission is additive on top of the baked light, so a very high value simply
/// saturates the surface to the brightest the framebuffer can show. The cap
/// keeps malformed or over-enthusiastic assets from producing infinities.
pub const MAX_EMISSION_INTENSITY: f32 = 8.0;

/// Emissive intensity used when a material authors an emissive colour but no
/// explicit intensity.
pub const DEFAULT_EMISSION_INTENSITY: f32 = 1.0;

/// A material's emission: colour, strength and optional mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialEmission {
    /// Emissive colour, each channel between zero and [`MAX_EMISSION_COLOR`].
    pub color: [f32; 3],
    /// Scalar multiplier between zero and [`MAX_EMISSION_INTENSITY`].
    pub intensity: f32,
    /// Optional emissive mask, expressed as an index into whichever texture
    /// table the owner uses (the material table for level materials, the model
    /// texture list for GLB props). `None` means the surface's own texture
    /// shapes the emission.
    pub mask: Option<u16>,
}

impl MaterialEmission {
    /// A non-emissive material: the default for every material that does not
    /// author emission, and the state old assets keep.
    pub const NONE: Self = Self {
        color: [0.0, 0.0, 0.0],
        intensity: 0.0,
        mask: None,
    };

    /// An unmasked emissive colour at the given intensity.
    #[must_use]
    pub const fn new(color: [f32; 3], intensity: f32) -> Self {
        Self {
            color,
            intensity,
            mask: None,
        }
    }

    /// The same emission restricted to a mask texture.
    #[must_use]
    pub const fn with_mask(self, mask: Option<u16>) -> Self {
        Self { mask, ..self }
    }

    /// True when the material actually emits: a positive strength on a colour
    /// that is not black.
    #[must_use]
    pub const fn is_emissive(&self) -> bool {
        self.intensity > 0.0 && (self.color[0] > 0.0 || self.color[1] > 0.0 || self.color[2] > 0.0)
    }

    /// The value a renderer uploads: colour premultiplied by intensity.
    #[must_use]
    pub const fn effective_color(&self) -> [f32; 3] {
        [
            self.color[0] * self.intensity,
            self.color[1] * self.intensity,
            self.color[2] * self.intensity,
        ]
    }

    /// Clamps every value into the documented range, mapping non-finite input
    /// to "no emission" rather than to a NaN a shader would spread.
    #[must_use]
    pub fn sanitized(self) -> Self {
        let channel = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, MAX_EMISSION_COLOR)
            } else {
                0.0
            }
        };
        let intensity = if self.intensity.is_finite() {
            self.intensity.clamp(0.0, MAX_EMISSION_INTENSITY)
        } else if self.intensity.is_sign_positive() {
            MAX_EMISSION_INTENSITY
        } else {
            0.0
        };
        Self {
            color: [
                channel(self.color[0]),
                channel(self.color[1]),
                channel(self.color[2]),
            ],
            intensity,
            mask: self.mask,
        }
    }
}

impl Default for MaterialEmission {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests;

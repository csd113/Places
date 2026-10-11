//! Pigment fluorescence excited only by ultraviolet fixtures.
/// Largest accepted pigment response. This never contributes light transport.
pub const MAX_UV_RESPONSE: f32 = 2.0;

/// Optional numeric response mask in the owner's texture list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialFluorescence {
    pub strength: f32,
    pub mask: Option<u32>,
    /// Linear authored material tint, kept separate from baked vertex lighting.
    pub tint: [f32; 3],
}

impl MaterialFluorescence {
    pub const NONE: Self = Self {
        strength: 0.0,
        mask: None,
        tint: [1.0; 3],
    };

    #[must_use]
    pub fn sanitized(self) -> Self {
        let strength = if self.strength.is_finite() {
            self.strength.clamp(0.0, MAX_UV_RESPONSE)
        } else {
            0.0
        };
        Self {
            strength,
            mask: if strength > 0.0 { self.mask } else { None },
            tint: if strength > 0.0 {
                self.tint.map(|channel| {
                    if channel.is_finite() {
                        channel.clamp(0.0, 1.0)
                    } else {
                        1.0
                    }
                })
            } else {
                [1.0; 3]
            },
        }
    }
}

impl Default for MaterialFluorescence {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_response_defaults_to_white_and_invalid_tint_sanitizes() {
        assert_eq!(MaterialFluorescence::default(), MaterialFluorescence::NONE);
        assert_eq!(
            MaterialFluorescence::NONE.tint.map(f32::to_bits),
            [1.0_f32.to_bits(); 3]
        );
        let response = MaterialFluorescence {
            strength: 1.0,
            mask: Some(0),
            tint: [f32::NAN, -1.0, 2.0],
        }
        .sanitized();
        assert_eq!(
            response.tint.map(f32::to_bits),
            [1.0_f32.to_bits(), 0.0_f32.to_bits(), 1.0_f32.to_bits()]
        );
        assert_eq!(
            MaterialFluorescence {
                strength: 0.0,
                ..response
            }
            .sanitized(),
            MaterialFluorescence::NONE
        );
    }
}

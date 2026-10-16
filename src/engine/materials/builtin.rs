//! Fixed engine sheets embedded from their catalogued PNG files.

use super::{RawImage, decode_png};

/// The closed set of repository images required by infallible engine accessors.
/// External images never enter this path; their decoder errors are propagated.
#[derive(Clone, Copy)]
pub enum BuiltinImage {
    FontAtlas,
    DecalAtlas,
    MissingTexture,
    EmergencyWhite,
}

impl BuiltinImage {
    /// Decodes an immutable build resource. Tests pin every dimension and RGBA
    /// byte against the reviewed sheets, so a failure identifies a
    /// broken repository build rather than malformed user content.
    #[expect(
        clippy::expect_used,
        reason = "Only this closed set of immutable include_bytes PNGs can reach the decoder; tests validate their dimensions and complete pixel hashes before shipping."
    )]
    pub(crate) fn decode(self) -> RawImage {
        let bytes: &[u8] = match self {
            Self::FontAtlas => include_bytes!("../../../assets/shared/ui/font_01.png"),
            Self::DecalAtlas => {
                include_bytes!("../../../assets/shared/decals/validation_atlas_01.png")
            }
            Self::MissingTexture => {
                include_bytes!("../../../assets/shared/textures/missing_01.png")
            }
            Self::EmergencyWhite => {
                include_bytes!("../../../assets/shared/textures/white_fallback_01.png")
            }
        };
        decode_png(bytes).expect("the embedded engine PNG is a validated repository resource")
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::BuiltinImage;

    #[test]
    fn embedded_sheets_preserve_reviewed_pixels_and_dimensions() {
        let cases = [
            (
                BuiltinImage::FontAtlas,
                256_u32,
                128_u32,
                "090bbdf60ce43699b56cea60bb6365643f1e92717b5c1bce4a5e66c440cb0406",
            ),
            (
                BuiltinImage::DecalAtlas,
                256_u32,
                256_u32,
                "8918fe526dfc6b0dfe5fee504a7ecc6b62ebd4a807f9705e45461c1c86f687cc",
            ),
            (
                BuiltinImage::MissingTexture,
                64_u32,
                64_u32,
                "040a0acf78f180c289eeeb103ac10007afddd252d8bf71666b64cc118ba30208",
            ),
            (
                BuiltinImage::EmergencyWhite,
                2_u32,
                2_u32,
                "5ac6a5945f16500911219129984ba8b387a06f24fe383ce4e81a73294065461b",
            ),
        ];
        for (image, width, height, digest) in cases {
            let decoded = image.decode();
            assert_eq!(
                (decoded.width, decoded.height),
                (width, height),
                "fixed sheet dimensions retain the existing atlas contract"
            );
            assert_eq!(
                format!("{:x}", Sha256::digest(&decoded.rgba)),
                digest,
                "every RGBA byte, including transparent RGB and row orientation, matches the reviewed image"
            );
        }
    }
}

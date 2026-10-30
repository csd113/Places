//! Fixed engine sheets embedded from their catalogued PNG files.

use super::{RawImage, decode_png};

/// The closed set of repository images required by infallible engine accessors.
/// External images never enter this path; their decoder errors are propagated.
#[derive(Clone, Copy)]
pub enum BuiltinImage {
    FontAtlas,
    EditorFont,
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
            Self::EditorFont => include_bytes!("../../../assets/shared/ui/editor_font.png"),
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
        // These fixed engine sheets are baseline resources, not cancellable
        // map input. Restore the caller's scope before its next checkpoint;
        // cancellation/admission errors must not become asset-invariant panics.
        let _engine_resource_scope = crate::engine::resources::enter(None);
        decode_png(bytes).expect("the embedded engine PNG is a validated repository resource")
    }
}

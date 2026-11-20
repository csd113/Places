//! Fixed interior accessibility for the reviewed Tall Guest model.

use crate::engine::materials::MaterialResponse;
use crate::engine::props::{LoadedPropAsset, reviewed_source_matches};

const CAVITY_VISIBILITY: &[u8] =
    include_bytes!("../../../assets/entities/tall_guest/model/cavity_visibility.bin");
const INTERIOR_MATERIAL: u16 = 1;

pub struct CharacterCavity(Option<&'static [u8]>);

impl CharacterCavity {
    pub fn new(asset: &LoadedPropAsset, real_self_shadows: bool) -> Self {
        Self((!real_self_shadows && Self::supports(asset)).then_some(CAVITY_VISIBILITY))
    }

    /// The model loader enforces the accepted export hash on the source bytes.
    pub fn supports(asset: &LoadedPropAsset) -> bool {
        asset.model_path == "entities/tall_guest/model/tall_guest.glb"
            && reviewed_source_matches(
                &asset.model_path,
                crate::engine::tall_guest::prey::HUNTER_SHA256,
            )
            && asset.model.vertices.len() == CAVITY_VISIBILITY.len()
    }

    pub fn attenuate(&self, vertex: usize, [red, green, blue, alpha]: [f32; 4]) -> [f32; 4] {
        let cavity = self
            .0
            .and_then(|values| values.get(vertex))
            .map_or(1.0, |value| f32::from(*value) / 255.0);
        [red * cavity, green * cavity, blue * cavity, alpha]
    }

    pub const fn material_response(
        &self,
        material: u16,
        response: MaterialResponse,
    ) -> MaterialResponse {
        if self.0.is_some() && material == INTERIOR_MATERIAL {
            MaterialResponse {
                specular: [0.0; 3],
                roughness: 1.0,
                ..response
            }
        } else {
            response
        }
    }
}

//! Physical traction of a material, independent of its visual response.

/// Ground locomotion contract. Omitted material properties preserve ordinary
/// walking; ice uses bounded acceleration and reduced braking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundSurface {
    #[default]
    Normal,
    Ice,
}

impl GroundSurface {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "normal" => Some(Self::Normal),
            "ice" => Some(Self::Ice),
            _ => None,
        }
    }
}

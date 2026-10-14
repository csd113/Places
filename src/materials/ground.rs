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
    /// Default rigid-body contact friction, independent of player locomotion.
    #[must_use]
    pub const fn default_friction(self) -> f32 {
        match self {
            Self::Normal => 0.7,
            Self::Ice => 0.025,
        }
    }

    /// Resolves an optional coefficient while preserving safe pack defaults.
    pub(crate) fn resolve_friction(self, authored: Option<f32>) -> f32 {
        authored
            .filter(|value| value.is_finite() && (0.0..=2.0).contains(value))
            .unwrap_or_else(|| self.default_friction())
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "normal" => Some(Self::Normal),
            "ice" => Some(Self::Ice),
            _ => None,
        }
    }
}

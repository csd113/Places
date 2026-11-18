//! Source identity for an individually replaceable prepared direct term.

use super::{LevelLighting, probes::ProbeField};

/// A fixture source that was separated from the residual in the prepared field.
#[derive(Clone, Copy)]
pub struct PreparedSource {
    pub source_id: usize,
    pub slot: usize,
}

impl PreparedSource {
    /// An unselected or multi-aperture fixture does not own one direct slot.
    pub fn bind(field: &ProbeField, lighting: &LevelLighting, fixture: usize) -> Option<Self> {
        let _separated_direct_field = field.local_direct.as_ref()?;
        let sources = lighting.fixture_light_indices(fixture);
        let &[source_id] = sources.as_slice() else {
            return None;
        };
        let packed_id = u32::try_from(source_id).ok()?;
        let slot = field
            .runtime_direct_lights()
            .iter()
            .position(|id| *id == packed_id)?;
        Some(Self { source_id, slot })
    }
}

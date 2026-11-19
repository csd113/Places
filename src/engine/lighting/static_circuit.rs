//! Offline preparation of the optional Static circuit receiver state.

use super::LevelLighting;
use crate::engine::level::{LevelDef, StaticSwitchMode};

pub struct StaticCircuitLighting {
    pub fixtures: Vec<usize>,
    pub unpowered: LevelLighting,
}

impl LevelLighting {
    /// Resolves the alternate against the identical authored light field. The
    /// clone shares immutable visibility and changes only circuit source flags.
    pub(crate) fn prepare_static_circuit(
        &self,
        level: &LevelDef,
    ) -> Result<Option<StaticCircuitLighting>, String> {
        let fixtures = Self::prepared_static_fixtures(level);
        if fixtures.is_empty() {
            return Ok(None);
        }
        let mut unpowered = self.clone();
        for fixture in &fixtures {
            let sources = self.fixture_light_indices(*fixture);
            if sources.is_empty() {
                return Err("Static circuit fixture has no resolved light".into());
            }
            for source in sources {
                let _changed = unpowered.set_light_enabled(source, false);
            }
        }
        Ok(Some(StaticCircuitLighting {
            fixtures,
            unpowered,
        }))
    }

    pub(crate) fn prepared_static_fixtures(level: &LevelDef) -> Vec<usize> {
        level
            .ceiling_lights
            .iter()
            .enumerate()
            .filter_map(|(index, fixture)| {
                (fixture.static_switch == StaticSwitchMode::Prepared).then_some(index)
            })
            .collect()
    }
}

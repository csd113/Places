//! Source membership for prepared contributions and runtime prop bindings.

use std::collections::BTreeMap;

use super::LevelLighting;
use super::entity_receivers::select_baked_direct_lights;
use super::transport::TransportEmitter;
use crate::engine::level::{LevelDef, PropDef};

/// Keep the resolved ordinal of every authored-enabled prop source. The
/// controller can be initially off without removing those source slots.
pub fn prop_source_members<'a>(
    lighting: &'a LevelLighting,
    prop_index: usize,
    prop: &'a PropDef,
) -> impl Iterator<Item = usize> + 'a {
    lighting
        .prop_light_indices(prop_index)
        .zip(&prop.lights)
        .filter_map(|(source, light)| light.enabled.then_some(source))
}

/// Two views of one traversal. The flat list reserves direct slots in fixture
/// then prop order. Named groups can interleave those members, but the first
/// owner's source remains the layer ID and group order never changes.
pub struct SwitchableSources {
    pub indices: Vec<usize>,
    pub groups: Vec<(usize, Vec<usize>)>,
}

impl SwitchableSources {
    pub fn from_level<'a>(level: &'a LevelDef, lighting: &LevelLighting) -> Self {
        let mut result = Self {
            indices: Vec::new(),
            groups: Vec::new(),
        };
        let mut named = BTreeMap::<&str, usize>::new();
        let mut add_group = |name: Option<&'a str>, members: Vec<usize>| {
            let Some(first) = members.first().copied() else {
                return;
            };
            if let Some(group) = name
                .and_then(|id| named.get(id))
                .and_then(|ordinal| result.groups.get_mut(*ordinal))
            {
                group.1.extend(members);
            } else {
                if let Some(id) = name {
                    let _previous = named.insert(id, result.groups.len());
                }
                result.groups.push((first, members));
            }
        };
        for (index, fixture) in level.ceiling_lights.iter().enumerate() {
            if fixture.switchable {
                let members = lighting.fixture_light_indices(index);
                for source in &members {
                    if !result.indices.contains(source) {
                        result.indices.push(*source);
                    }
                }
                add_group(fixture.switch_group.as_deref(), members);
            }
        }
        for (index, prop) in level.props.iter().enumerate() {
            if prop
                .light_control()
                .is_some_and(|control| control.switchable)
            {
                let members: Vec<_> = prop_source_members(lighting, index, prop).collect();
                result.indices.extend(members.iter().copied());
                add_group(prop.switch_group.as_deref(), members);
            }
        }
        result
    }
}

/// Ordered offline emitters and the index mapping for reserved runtime slots.
pub struct Emitters {
    pub sources: Vec<TransportEmitter>,
    pub runtime_direct: Vec<(usize, u32)>,
    pub groups: Vec<(usize, Vec<usize>)>,
    pub switchable_count: usize,
}

pub fn prepare_emitters(level: &LevelDef, lighting: &LevelLighting) -> Emitters {
    let mut emitters: Vec<TransportEmitter> = Vec::new();
    let mut switchable_emitters = 0_usize;
    let switchable = SwitchableSources::from_level(level, lighting);
    let selected = select_baked_direct_lights(lighting, &switchable.indices);
    let mut runtime_direct = Vec::new();
    for (index, light) in lighting.lights().iter().enumerate() {
        let tag = switchable
            .indices
            .iter()
            .copied()
            .find(|candidate| *candidate == index);
        // A switchable fixture is prepared even when it is authored off, so a
        // runtime toggle can turn its contribution on; a plainly inactive
        // light contributes nothing to any solve.
        if !light.is_active() && tag.is_none() {
            continue;
        }
        if let Ok(source) = u32::try_from(index)
            && selected.contains(&source)
        {
            runtime_direct.push((emitters.len(), source));
        }
        emitters.push(TransportEmitter::from_baked(light, tag));
        if tag.is_some() {
            switchable_emitters = switchable_emitters.saturating_add(1);
        }
    }
    Emitters {
        sources: emitters,
        runtime_direct,
        groups: switchable.groups,
        switchable_count: switchable_emitters,
    }
}

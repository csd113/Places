//! One previous editor build supplies immutable batches; no second geometry cache.
use std::collections::HashMap;

use super::api::LevelBuild;
use super::props::PropMeshBatch;

#[derive(Default)]
pub struct PreviewDependencies {
    pub lighting: Vec<u8>,
    pub batches: Vec<String>,
    pub reused_batches: usize,
    pub reused_mesh: bool,
    pub equivalent_lighting: bool,
}

#[derive(Clone, Copy, Default)]
pub struct PreviewReuse<'a> {
    pub inspect: Option<&'a [PropMeshBatch]>,
    pub previous: Option<(&'a LevelBuild, &'a PreviewDependencies)>,
    pub same_architecture: bool,
}

pub(super) struct PropReuse<'a> {
    previous: HashMap<&'a str, &'a PropMeshBatch>,
    pub next: Vec<String>,
    pub reused: usize,
    pub reused_indices: Vec<usize>,
}

impl<'a> PropReuse<'a> {
    pub fn new(previous: Option<(&'a [String], &'a [PropMeshBatch])>) -> Self {
        Self {
            previous: previous
                .into_iter()
                .flat_map(|(keys, batches)| keys.iter().zip(batches))
                .filter(|(key, _)| !key.is_empty())
                .map(|(key, batch)| (key.as_str(), batch))
                .collect(),
            next: Vec::new(),
            reused: 0,
            reused_indices: Vec::new(),
        }
    }

    pub fn batch(&mut self, key: String) -> Option<PropMeshBatch> {
        let previous = self.previous.get(key.as_str()).copied();
        self.next.push(key);
        if previous.is_some() {
            self.reused = self.reused.saturating_add(1);
            self.reused_indices.push(self.next.len().saturating_sub(1));
        }
        previous.cloned()
    }
}

pub(super) fn finish_dependencies(
    reuse: PreviewReuse<'_>,
    lighting: Option<Vec<u8>>,
    props: PropReuse<'_>,
    state: (bool, bool),
) -> PreviewDependencies {
    PreviewDependencies {
        lighting: lighting.unwrap_or_default(),
        batches: if reuse.inspect.is_some() {
            reuse
                .previous
                .map_or_else(Vec::new, |(_, old)| old.batches.clone())
        } else {
            props.next
        },
        reused_batches: props.reused,
        reused_mesh: state.0,
        equivalent_lighting: state.1,
    }
}

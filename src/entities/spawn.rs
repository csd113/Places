//! Typed spawn templates, spawn points and at-most-one-active spawn groups.
//!
//! Spawning is authored, never scripted: a [`SpawnTemplateDef`] is a typed
//! prefab (a model, a scale, an optional lifetime and the components and
//! bindings the instance is born with), and a [`SpawnPointDef`] says where that
//! template appears. `spawn_entity` names a point (and optionally a template,
//! group or runtime name); `despawn_entity` names an entity, a group or the
//! runtime name.
//!
//! A [`SpawnGroupDef`] with `at_most_one_active` is the reusable encounter
//! mechanism: while one member of the group is alive a second spawn into the
//! group is refused with a diagnostic instead of stacking instances. Group
//! membership is released by the member's own despawn (lifetime expiry,
//! `despawn_entity`, or a level reset), so a group can never leak a phantom
//! member.
//!
//! The world owns the spawned entity records and their components; this module
//! owns the authored definitions and the group bookkeeping.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::id::EntityHandle;
use crate::level::{ComponentDef, EventBindingDef};

/// One authored spawn template: the typed prefab an action instantiates.
///
/// ```json
/// { "id": "steam_rat", "model": "core:cardboard_box", "scale": 0.5,
///   "lifetime_seconds": 30.0,
///   "components": [{ "component": "state", "name": "phase", "value": "idle" }],
///   "bindings": [{ "on": "spawn", "actions": [{ "action": "set_state",
///                   "name": "phase", "value": "active" }] }] }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnTemplateDef {
    /// Stable template id, unique per level.
    pub id: String,
    /// Registry model id or catalogue model path the instance draws.
    pub model: String,
    /// Uniform scale; defaults to `1.0`.
    #[serde(default = "default_spawn_scale")]
    pub scale: f32,
    /// Seconds the instance lives before despawning itself; omitted lives
    /// until an action or a reset removes it.
    #[serde(default)]
    pub lifetime_seconds: Option<f32>,
    /// Components every instance is born with.
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    /// Bindings every instance is born with.
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
}

const fn default_spawn_scale() -> f32 {
    1.0
}

/// One authored spawn point: where a template appears.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnPointDef {
    /// Stable point id, unique per level.
    pub id: String,
    /// World X.
    #[serde(default)]
    pub x: f32,
    /// World Y of the spawned base; omitted resolves the walkable floor under
    /// the point.
    #[serde(default)]
    pub y: Option<f32>,
    /// World Z.
    #[serde(default)]
    pub z: f32,
    /// Spawn yaw in degrees.
    #[serde(default)]
    pub yaw_degrees: f32,
    /// Template this point instantiates.
    pub template: String,
    /// Group this point's spawns belong to; omitted means ungrouped.
    #[serde(default)]
    pub group: Option<String>,
    /// What this point's own events do (for example `on: spawn`).
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
}

/// One authored spawn group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnGroupDef {
    /// Stable group id, unique per level.
    pub id: String,
    /// While one member is alive, a second spawn into this group is refused.
    #[serde(default)]
    pub at_most_one_active: bool,
}

/// Runtime state of one spawn group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnGroupRuntime {
    /// The authored definition.
    pub def: SpawnGroupDef,
    /// The live member, if any.
    pub live: Option<EntityHandle>,
    /// Spawns this group has admitted since the last reset.
    pub spawns: u64,
}

/// Every spawn group of one level.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnGroups {
    groups: Vec<SpawnGroupRuntime>,
    by_id: HashMap<String, usize>,
}

impl SpawnGroups {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            by_id: HashMap::new(),
        }
    }

    /// Resolves the level's authored spawn groups.
    #[must_use]
    pub fn from_level(level: &crate::level::LevelDef) -> Self {
        let mut groups = Vec::new();
        let mut by_id = HashMap::new();
        for def in &level.spawn_groups {
            let id = def.id.trim();
            if id.is_empty() || by_id.contains_key(id) {
                continue;
            }
            let _previous_value = by_id.insert(id.to_string(), groups.len());
            groups.push(SpawnGroupRuntime {
                def: def.clone(),
                live: None,
                spawns: 0,
            });
        }
        Self { groups, by_id }
    }

    /// Number of groups.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.groups.len()
    }

    /// True when the level authors no group.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// One group's runtime by authored id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SpawnGroupRuntime> {
        self.by_id
            .get(id.trim())
            .and_then(|index| self.groups.get(*index))
    }

    /// The index of the group called `id`, if any.
    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id.trim()).copied()
    }

    /// One group's runtime by index.
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&SpawnGroupRuntime> {
        self.groups.get(index)
    }

    /// Every group, in authored order.
    pub fn iter(&self) -> std::slice::Iter<'_, SpawnGroupRuntime> {
        self.groups.iter()
    }

    /// True when a spawn into `index` may proceed.
    ///
    /// `at_most_one_active` refuses while a member is live; every other group
    /// admits any number of members.
    #[must_use]
    pub fn admits(&self, index: usize) -> bool {
        self.groups
            .get(index)
            .is_some_and(|group| !group.def.at_most_one_active || group.live.is_none())
    }

    /// Records `handle` as a live member of `index`.
    pub fn occupy(&mut self, index: usize, handle: EntityHandle) -> bool {
        let Some(group) = self.groups.get_mut(index) else {
            return false;
        };
        if group.def.at_most_one_active && group.live.is_some() {
            return false;
        }
        if group.live.is_none() {
            group.live = Some(handle);
        }
        group.spawns = group.spawns.saturating_add(1);
        true
    }

    /// Releases every group whose live member is `handle`.
    ///
    /// Returns how many groups were released; a despawn of a non-member
    /// releases nothing.
    pub fn release(&mut self, handle: EntityHandle) -> usize {
        let mut released = 0usize;
        for group in &mut self.groups {
            if group.live == Some(handle) {
                group.live = None;
                released = released.saturating_add(1);
            }
        }
        released
    }

    /// Every group whose live member is `handle`.
    #[must_use]
    pub fn groups_of(&self, handle: EntityHandle) -> Vec<usize> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.live == Some(handle))
            .map(|(index, _)| index)
            .collect()
    }

    /// Drops every live membership and spawn counter.
    pub fn reset(&mut self) {
        for group in &mut self.groups {
            group.live = None;
            group.spawns = 0;
        }
    }
}

impl<'a> IntoIterator for &'a SpawnGroups {
    type Item = &'a SpawnGroupRuntime;
    type IntoIter = std::slice::Iter<'a, SpawnGroupRuntime>;

    fn into_iter(self) -> Self::IntoIter {
        self.groups.iter()
    }
}

/// The maximum spawn templates one level may author.
///
/// Raised to 256 from 64: a template is one small authored record cloned at
/// spawn, and the live-spawn budget below (not the template list) bounds what
/// can exist at once.
pub const MAX_LEVEL_SPAWN_TEMPLATES: usize = 256;

/// The maximum spawn points one level may author.
///
/// Raised to 1024 from 256: a spawn point is a placement record; at most
/// [`MAX_LIVE_SPAWNS`] spawned entities exist at once.
pub const MAX_LEVEL_SPAWN_POINTS: usize = 1024;

/// The maximum spawn groups one level may author.
///
/// Raised to 256 from 64, matching the template budget.
pub const MAX_LEVEL_SPAWN_GROUPS: usize = 256;

/// The maximum number of entities spawned by actions and alive at once.
///
/// Raised to 512 from 128. Each live spawn is one entity handle plus the
/// runtime object the template instantiates; spawned skinned actors draw from
/// the shared [`crate::render::MAX_CHARACTERS`] budget, so this list is the
/// gameplay-side bound and characters are the render-side one.
pub const MAX_LIVE_SPAWNS: usize = 512;

/// The maximum number of spawn requests one tick may apply.
///
/// Raised to 64 from 16: a tick that runs a sequence pushing a burst of
/// spawns should drain the burst rather than half of it, and 64 instantiations
/// per tick is bounded, deterministic work.
pub const MAX_SPAWNS_PER_TICK: usize = 64;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::level::LevelDef;

    fn handle(index: u32) -> EntityHandle {
        EntityHandle::from_parts(index, 1)
    }

    fn level_with_groups(json: &str) -> LevelDef {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "spawn_test",
                "name": "Spawn Test",
                "spawn": {{ "x": 1.0, "z": 1.0 }},
                "rooms": [ {{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }} ],
                "spawn_groups": {json}
            }}"#
        ))
        .expect("the spawn test level parses")
    }

    #[test]
    fn an_at_most_one_group_admits_one_member_until_released() {
        let level = level_with_groups(
            r#"[ { "id": "rats", "at_most_one_active": true },
                 { "id": "props" } ]"#,
        );
        let mut groups = SpawnGroups::from_level(&level);
        assert_eq!(groups.len(), 2);
        let rats = groups.index_of("rats").expect("the rats group");
        assert!(groups.admits(rats));
        assert!(groups.occupy(rats, handle(1)));
        assert!(!groups.admits(rats), "one member is already live");
        assert!(
            !groups.occupy(rats, handle(2)),
            "a second member is refused"
        );
        assert_eq!(groups.at(rats).map(|group| group.spawns), Some(1));
        assert_eq!(groups.release(handle(1)), 1);
        assert!(groups.admits(rats), "released, so a new member may spawn");
        assert!(groups.occupy(rats, handle(3)));
    }

    #[test]
    fn an_unbounded_group_admits_many_members_and_releases_only_its_own() {
        let level = level_with_groups(r#"[ { "id": "props" } ]"#);
        let mut groups = SpawnGroups::from_level(&level);
        let props = groups.index_of("props").expect("the props group");
        assert!(groups.occupy(props, handle(1)));
        assert!(groups.occupy(props, handle(2)));
        assert_eq!(groups.release(handle(1)), 1);
        assert_eq!(groups.release(handle(1)), 0, "already released");
        assert_eq!(groups.at(props).map(|group| group.spawns), Some(2));
    }

    #[test]
    fn duplicate_and_empty_group_ids_are_skipped() {
        let level = level_with_groups(r#"[ { "id": "a" }, { "id": "a" }, { "id": "  " } ]"#);
        let groups = SpawnGroups::from_level(&level);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.index_of("a"), Some(0));
        assert_eq!(groups.index_of("missing"), None);
    }

    #[test]
    fn reset_drops_membership_without_forgetting_the_groups() {
        let level = level_with_groups(r#"[ { "id": "rats", "at_most_one_active": true } ]"#);
        let mut groups = SpawnGroups::from_level(&level);
        let rats = groups.index_of("rats").expect("group");
        assert!(groups.occupy(rats, handle(1)));
        groups.reset();
        assert!(groups.admits(rats));
        assert_eq!(groups.at(rats).and_then(|group| group.live), None);
        assert_eq!(groups.groups_of(handle(1)), Vec::<usize>::new());
    }
}

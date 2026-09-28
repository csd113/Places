//! Entity identity: authored ids, generation-safe runtime handles and the
//! store that owns entity slots.
//!
//! Authored relationships (a binding's `target`, a sequence's actor, a spawn
//! group's point) always name a stable [`EntityId`]. Runtime relationships
//! (a queued event, a running sequence, a spawn group member) carry an
//! [`EntityHandle`], which is only valid while the slot's generation matches
//! the one the handle was issued with. Removing an entity or clearing the
//! store for a new world therefore invalidates every old handle at once: a
//! queued event from a previous world can never address a new world's
//! coincidentally equal slot.

use std::collections::HashMap;
use std::fmt;

/// A stable, authored entity id.
///
/// This is the level's own instance namespace — the same ids
/// `LevelDef::prop_instance_ids` resolves — so a map relationship never depends
/// on array order, allocation order or a renderer handle.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(String);

impl EntityId {
    /// Wraps a validated authored id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when the id is empty after trimming.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for EntityId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for EntityId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A generation-checked runtime reference to one entity slot.
///
/// Handles are eight bytes of plain data; they are deliberately not pointers
/// and not array indices, so an authored relationship never depends on
/// transient allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityHandle {
    index: u32,
    generation: u32,
}

impl EntityHandle {
    /// Rebuilds a handle from a slot index and the generation the slot held.
    ///
    /// Used by the component tables, which record each entry's generation
    /// alongside its value so iteration can yield a handle without consulting
    /// the store.
    #[must_use]
    pub(crate) const fn from_parts(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// The slot index this handle addresses.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// The slot generation the handle was issued with.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// A relationship written by a map (authored id) or by the runtime (handle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityRef<'a> {
    /// A stable authored id.
    Authored(&'a str),
    /// A live runtime handle.
    Handle(EntityHandle),
}

/// One store slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Slot {
    generation: u32,
    live: bool,
}

/// The generational slot store every component table is aligned to.
///
/// The store owns slot lifetime only; component data lives in
/// [`super::components::ComponentTables`], indexed by the same slot index. A
/// slot is reused only after [`EntityStore::remove`] returns it to the free
/// list, and reuse bumps the slot's generation, so a stale handle never
/// resolves to the new occupant.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityStore {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
    generation: u64,
}

impl EntityStore {
    /// An empty store at generation 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            generation: 0,
        }
    }

    /// Creates a slot and returns its handle.
    pub fn insert(&mut self) -> EntityHandle {
        if let Some(index) = self.free.pop()
            && let Some(slot) = self.slots.get_mut(index as usize)
        {
            slot.live = true;
            self.live = self.live.saturating_add(1);
            return EntityHandle {
                index,
                generation: slot.generation,
            };
        }
        // A store with more than `u32::MAX` slots cannot address one; the
        // engine's own level budgets are orders of magnitude below this.
        let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
        self.slots.push(Slot {
            generation: 0,
            live: true,
        });
        self.live = self.live.saturating_add(1);
        EntityHandle {
            index,
            generation: 0,
        }
    }

    /// Drops one slot and returns whether it was live.
    ///
    /// The slot's generation is bumped so every handle issued before this call
    /// resolves to `None`.
    pub fn remove(&mut self, handle: EntityHandle) -> bool {
        let Some(slot) = self.slots.get_mut(handle.index as usize) else {
            return false;
        };
        if !slot.live || slot.generation != handle.generation {
            return false;
        }
        slot.live = false;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(handle.index);
        self.live = self.live.saturating_sub(1);
        true
    }

    /// True when `handle` addresses a live slot at the handle's generation.
    #[must_use]
    pub fn contains(&self, handle: EntityHandle) -> bool {
        self.slots
            .get(handle.index as usize)
            .is_some_and(|slot| slot.live && slot.generation == handle.generation)
    }

    /// Number of live slots.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.live
    }

    /// True when no slot is live.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// The world generation this store was last cleared at.
    ///
    /// Every queued runtime reference records it, so a record produced before
    /// a level replacement is discarded instead of being resolved against the
    /// new world.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Drops every slot and advances the world generation.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.live = 0;
        self.generation = self.generation.wrapping_add(1);
    }

    /// The live handles, in slot order.
    #[must_use]
    pub fn handles(&self) -> Vec<EntityHandle> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.live)
            .filter_map(|(index, slot)| {
                u32::try_from(index).ok().map(|index| EntityHandle {
                    index,
                    generation: slot.generation,
                })
            })
            .collect()
    }

    /// Number of allocated slots, live or free. Test and diagnostic use.
    #[must_use]
    pub const fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

/// The authored id table: one [`EntityHandle`] per unique authored id.
#[derive(Clone, Debug, Default)]
pub struct EntityNames {
    by_id: HashMap<EntityId, EntityHandle>,
    /// Reverse lookup: one entry per live binding, so resolving a handle's
    /// authored id on the action path is a hash lookup, not a scan.
    by_handle: HashMap<EntityHandle, EntityId>,
}

impl EntityNames {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Binds `id` to `handle`. Returns the previous binding, if any.
    pub fn insert(&mut self, id: EntityId, handle: EntityHandle) -> Option<EntityHandle> {
        if let Some(previous) = self.by_handle.remove(&handle) {
            self.by_id.remove(&previous);
        }
        let previous = self.by_id.insert(id.clone(), handle);
        if let Some(previous_handle) = previous {
            self.by_handle.remove(&previous_handle);
        }
        self.by_handle.insert(handle, id);
        previous
    }

    /// The handle currently bound to `id`, if the binding is still live.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<EntityHandle> {
        self.by_id.get(&EntityId::new(id)).copied()
    }

    /// The authored id bound to `handle`, if any.
    #[must_use]
    pub fn id_of(&self, handle: EntityHandle) -> Option<&EntityId> {
        self.by_handle.get(&handle)
    }

    /// Removes one binding.
    pub fn remove(&mut self, id: &str) -> Option<EntityHandle> {
        let handle = self.by_id.remove(&EntityId::new(id))?;
        self.by_handle.remove(&handle);
        Some(handle)
    }

    /// Number of bindings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// True when no binding exists.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// True when `id` already has a binding.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.by_id.contains_key(&EntityId::new(id))
    }

    /// Drops every binding.
    pub fn clear(&mut self) {
        self.by_id.clear();
        self.by_handle.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;

    #[test]
    fn a_removed_slot_never_resolves_again_even_when_reused() {
        let mut store = EntityStore::new();
        let first = store.insert();
        assert!(store.contains(first));
        assert!(store.remove(first));
        assert!(!store.contains(first), "a removed handle is stale");
        let second = store.insert();
        assert_eq!(second.index(), first.index(), "the slot is reused");
        assert_ne!(
            second.generation(),
            first.generation(),
            "reuse bumps the generation"
        );
        assert!(!store.contains(first), "the stale handle stays stale");
        assert!(store.contains(second));
    }

    #[test]
    fn clearing_the_store_advances_the_world_generation() {
        let mut store = EntityStore::new();
        let handle = store.insert();
        let generation = store.generation();
        store.clear();
        assert_eq!(store.generation(), generation.wrapping_add(1));
        assert!(!store.contains(handle));
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn removing_twice_is_a_no_op_and_handles_lists_are_in_slot_order() {
        let mut store = EntityStore::new();
        let a = store.insert();
        let b = store.insert();
        let c = store.insert();
        assert!(store.remove(b));
        assert!(!store.remove(b));
        assert_eq!(store.len(), 2);
        assert_eq!(store.handles(), vec![a, c]);
    }

    #[test]
    fn names_bind_authored_ids_to_handles() {
        let mut names = EntityNames::new();
        let handle = EntityHandle {
            index: 3,
            generation: 1,
        };
        assert!(names.insert(EntityId::new("hall_switch"), handle).is_none());
        assert_eq!(names.get("hall_switch"), Some(handle));
        assert_eq!(names.get("missing"), None);
        assert_eq!(
            names.id_of(handle).map(EntityId::as_str),
            Some("hall_switch")
        );
        assert_eq!(names.len(), 1);
        assert!(names.contains("hall_switch"));
        assert_eq!(names.remove("hall_switch"), Some(handle));
        assert!(names.is_empty());
    }
}

//! Essential finite-identity boundaries: seed a two-slot store near numeric
//! exhaustion instead of billions of reuse operations. No assets or native work.
use super::{EntityHandle, EntityStore, Slot};

#[test]
fn exhausted_generations_never_reissue_an_old_identity() {
    let mut store = EntityStore::new();
    store.slots.push(Slot {
        generation: u32::MAX - 1,
        live: true,
    });
    store.live = 1;
    let old = EntityHandle::from_parts(0, u32::MAX - 1);
    assert!(store.remove(old));
    let last = store.insert();
    assert_eq!(last.generation(), u32::MAX);
    assert!(store.remove(last));
    let elsewhere = store.insert();
    assert_ne!(elsewhere.index(), last.index());
    assert!(!store.contains(old));
    assert!(!store.contains(last));
    store.clear();
    assert!(store.try_insert().is_none());
    assert_eq!(store.slot_count(), 0);
    store.clear();
    assert!(store.try_insert().is_none());
}

#[test]
fn world_generation_exhaustion_is_explicit_and_does_not_mutate() {
    let mut store = EntityStore::new();
    let current = store.insert();
    store.generation = u64::MAX;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.clear()));
    assert!(result.is_err());
    assert!(store.contains(current));
    assert_eq!(store.generation(), u64::MAX);
}

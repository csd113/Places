//! Essential RLA-01 regression: old runtime references must never address fresh
//! occupants after clear, including old hole/reuse generations. A two-slot store
//! and one metadata volume cover public world/component reads, writes and remove
//! without assets, native devices or map baking.
use crate::engine::entities::EntityWorld;
use crate::engine::entities::components::ComponentTable;
use crate::engine::entities::id::{EntityHandle, EntityStore};
use crate::engine::level::LevelDef;

#[test]
fn clears_invalidate_every_old_reuse_and_hole_handle() {
    assert_eq!(std::mem::size_of::<EntityHandle>(), 8);
    let mut store = EntityStore::new();
    let mut components = ComponentTable::default();
    let mut stale = Vec::new();
    for _ in 0_u8..6 {
        let first = store.insert();
        let hole = store.insert();
        stale.push(first);
        stale.push(hole);
        assert!(store.remove(first));
        let reused = store.insert();
        stale.push(reused);
        assert_eq!(first.index(), reused.index());
        assert!(store.remove(hole));
        store.clear();
        components.clear();
        let fresh = store.insert();
        assert!(components.insert(fresh, 23_u8).is_none());
        for &old in &stale {
            assert!(!store.contains(old));
            assert!(!store.remove(old));
            assert!(components.get(old).is_none());
            assert!(components.get_mut(old).is_none());
            assert!(components.remove(old).is_none());
        }
        assert_eq!(components.get(fresh), Some(&23));
        assert!(store.contains(fresh));
        stale.push(fresh);
        store.clear();
        store.clear();
    }
}

#[test]
fn public_world_lookup_and_components_reject_pre_resolve_handles() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"identity","name":"Identity","spawn":{"x":-2,"z":-2},"volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1}]}"#).map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let mut old = Vec::new();
    for _ in 0_u8..5 {
        old.push(world.handle_of("zone").ok_or("volume handle missing")?);
        let generation = world.generation();
        world.resolve(&level);
        assert!(world.generation() > generation);
        let fresh = world
            .handle_of("zone")
            .ok_or("fresh volume handle missing")?;
        for &stale in &old {
            assert_ne!(stale, fresh);
            assert!(world.id_of(stale).is_none());
            assert!(!world.entities().contains(stale));
            assert!(world.components().volumes.get(stale).is_none());
            assert!(world.components_mut().volumes.get_mut(stale).is_none());
            assert!(world.components_mut().volumes.remove(stale).is_none());
        }
        assert_eq!(world.id_of(fresh), Some("zone"));
        assert!(world.components().volumes.get(fresh).is_some());
    }
    Ok(())
}

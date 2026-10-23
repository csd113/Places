//! Deleting an authored rigid prop must remove its entity identity as well as
//! its native body. Two passive metadata-only props exercise generated and
//! trimmed authored ids, stale events and runtime resets in milliseconds,
//! without loading assets, allocating a GPU or baking a map.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::events::EventKind;
use crate::engine::entities::id::EntityHandle;
use crate::engine::entities::{EntityWorld, WorldContext};
use crate::engine::level::{LevelDef, WalkableFloor};

fn assert_removed(world: &EntityWorld, deleted: &[(String, EntityHandle)]) {
    for (id, handle) in deleted {
        assert!(
            world.handle_of(id).is_none(),
            "deleted id {id} must not resolve"
        );
        assert!(
            world.id_of(*handle).is_none(),
            "deleted handle must have no id"
        );
        assert!(
            !world.entities().contains(*handle),
            "deleted handles must be stale"
        );
        assert!(
            world.components().transforms.get(*handle).is_none(),
            "deleted handles must not retain transforms"
        );
        assert!(
            world.components().renderables.get(*handle).is_none(),
            "deleted handles must not retain render ownership"
        );
    }
    assert!(
        world.entities().is_empty(),
        "all fixture entities must be removed"
    );
    assert!(
        world.names().is_empty(),
        "the id table must not retain tombstone aliases"
    );
    assert!(
        world.components().is_empty(),
        "no deleted component may survive or return"
    );
    assert_eq!(
        world.binding_count(),
        0,
        "passive deleted props must have no active bindings"
    );
}

#[test]
fn authored_physics_deletion_resolves_ids_and_survives_pending_events_and_reset()
-> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
            "schema": "places-level", "format_version": 1,
            "id": "authored_physics_removal",
            "name": "Authored physics removal fixture",
            "spawn": { "x": 0.0, "z": 0.0 },
            "props": [
                { "model": "core:plant", "solid": true,
                  "physics": { "motion": "dynamic", "pickup": true } },
                { "id": "  named_plant  ", "model": "core:plant", "x": 1.0,
                  "solid": true, "physics": { "motion": "kinematic" } }
            ]
        }"#,
    )
    .map_err(|error| error.to_string())?;
    let ids = level.prop_instance_ids();
    assert_eq!(
        ids,
        [String::from("plant_1"), String::from("named_plant")],
        "native item bookkeeping must use the engine's generated and trimmed prop ids"
    );
    let mut world = EntityWorld::from_level(&level);
    let mut deleted = Vec::with_capacity(ids.len());
    for id in ids {
        let handle = world
            .handle_of(&id)
            .ok_or_else(|| format!("fixture id {id} did not resolve"))?;
        assert!(
            world.components().transforms.contains(handle),
            "the initial prop representation must exist"
        );
        assert!(
            world.emit(EventKind::Interact, handle, "", Some(handle)),
            "the fixture event must queue"
        );
        deleted.push((id, handle));
    }
    for (id, _) in &deleted {
        assert!(
            world.remove_authored_physics_entity(id),
            "resolved authored prop {id} must be removable"
        );
        assert!(
            !world.remove_authored_physics_entity(id),
            "removal must not target an already deleted id"
        );
    }
    assert_removed(&world, &deleted);

    let index = CollisionIndex::build(&[]);
    let floor = WalkableFloor::default();
    let context = WorldContext {
        delta_seconds: 0.0,
        feet_from: Vec3::ZERO,
        feet: Vec3::ZERO,
        eye: Vec3::new(0.0, 1.6, 0.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let tick = world.tick(&context);
    assert_eq!(
        tick.actions_run, 0,
        "events for removed subjects must be inert"
    );
    assert!(world.events().is_empty(), "stale pending events must drain");
    assert_removed(&world, &deleted);
    world.reset_runtime();
    assert_removed(&world, &deleted);
    assert!(
        world.take_commands().is_empty(),
        "pending work and reset must not respawn deleted props"
    );
    Ok(())
}

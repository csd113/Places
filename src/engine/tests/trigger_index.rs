//! A small sparse field protects broadphase selectivity, swept crossings and
//! disappearance cleanup together. Sixty-four metadata boxes cost <1 ms and
//! load no assets; existing `trigger_occupants` fixtures cover queue retry policy.

use glam::Vec3;

use crate::engine::entities::events::EventKind;
use crate::engine::entities::{EntityWorld, TriggerItem};
use crate::engine::level::{LevelDef, TriggerFilterDef};

#[test]
fn sparse_trigger_queries_preserve_crossings_and_missing_item_exits_without_scanning_the_field()
-> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"indexed","name":"Indexed","spawn":{"x":-10,"z":-10},"volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1}]}"#)
        .map_err(|error| error.to_string())?;
    let template = level.volumes.first().cloned().ok_or("missing volume")?;
    level.volumes.clear();
    for index in 0_u16..64 {
        let mut volume = template.clone();
        volume.id = Some(format!("zone_{index}"));
        volume.x = f32::from(index) * 20.0;
        volume.filter = TriggerFilterDef::Item {
            asset: None,
            instance: None,
            held: None,
        };
        level.volumes.push(volume);
    }
    let mut world = EntityWorld::from_level(&level);
    let outside = Vec3::splat(-10.0);
    world.seed_volumes(outside);
    let item = TriggerItem {
        key: 1,
        instance: "item",
        asset: "core:plant",
        from: Vec3::new(-2.0, 0.5, 0.5),
        to: Vec3::new(2.0, 0.5, 0.5),
        half_extents: Vec3::splat(0.05),
        held: false,
        path: None,
        sleeping: false,
        centre_path: None,
    };
    world.update_volume_occupants(outside, outside, &[item]);
    assert_eq!(
        world.events().len(),
        2,
        "full fast crossing retains both edges"
    );
    assert_eq!(
        world.trigger_query_stats().volumes_sampled,
        1,
        "distant authored volumes receive no exact tests"
    );
    assert_eq!(
        world.trigger_query_stats().item_sweeps,
        1,
        "item exact tests visit only a matching spatial candidate"
    );
    let _ = world.dispatch_queued_events();
    let resting = TriggerItem {
        from: Vec3::splat(0.5),
        to: Vec3::splat(0.5),
        ..item
    };
    world.seed_volume_occupants(outside, &[resting]);
    world.update_volumes(outside, outside);
    assert!(
        world
            .events()
            .peek()
            .is_some_and(|event| event.kind == EventKind::ExitVolume),
        "missing physical actor still exits its previously occupied remote volume"
    );
    assert_eq!(
        world.trigger_query_stats().volumes_sampled,
        1,
        "occupied candidate survives absence from all actor queries"
    );
    let _ = world.dispatch_queued_events();
    world.update_volumes(outside, outside);
    assert_eq!(
        world.trigger_query_stats().volumes_sampled,
        0,
        "cleared volume leaves the active set"
    );
    Ok(())
}

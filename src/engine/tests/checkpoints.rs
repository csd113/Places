//! Essential reconstruction regression: actual Luau closure locals and once-only
//! startup and full-precision numeric locals survive disk JSON and a fresh VM,
//! plus held item ownership survives fresh solver keys.
//! Existing scripting/hold tests cover live sessions, not reconstruction. Two tiny
//! CPU worlds and a sphere, no filesystem, model decode, render or map bake.
use crate::engine::{
    collision_index::CollisionIndex,
    entities::{EntityWorld, WorldContext},
    level::{ActionDef, LevelDef, WalkableFloor},
    physics::{BodyMotion, ItemWorld, PhysicsDef, ShapeDef},
    project::ProjectPath,
    scripting::ScriptSource,
};
use glam::Vec3;

#[test]
fn checkpoint_reconstructs_luau_locals_without_replaying_start() -> Result<(), String> {
    let mut level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"checkpoint_script","name":"Checkpoint","spawn":{"x":2,"z":2},"rooms":[{"width":4,"depth":4}],"props":[{"id":"quest","model":"core:chair","x":1,"z":1,"components":[{"component":"state","name":"count","value":0}]}]}"#).map_err(|problem|problem.to_string())?;
    let path = ProjectPath::new("scripts/quest.luau".into())?;
    level.script_sources.push(ScriptSource {path:path.clone(),source:"local count=0; local ratio=0.12345678901234567; return {start=function(api) api.set_state('quest','count',100) end, entered=function(api) if ratio ~= 0.12345678901234567 then error('Checkpoint lost number precision') end; count+=1; api.set_state('quest','count',count) end,checkpoint_state=function() return {count=count,ratio=ratio} end,restore_state=function(saved) count=saved.count; ratio=saved.ratio end}".into()});
    let mut original = EntityWorld::from_level(&level);
    tick(&mut original, &level);
    let _request = original.dispatch_on(
        "quest",
        &[ActionDef::RunScript {
            script: path.clone(),
            handler: "entered".into(),
        }],
    );
    tick(&mut original, &level);
    let encoded =
        serde_json::to_vec(&original.checkpoint()?).map_err(|problem| problem.to_string())?;
    let snapshot = serde_json::from_slice(&encoded).map_err(|problem| problem.to_string())?;
    let mut restored = EntityWorld::from_level(&level);
    restored.restore_checkpoint(&snapshot)?;
    let _publication = restored.take_commands();
    tick(&mut restored, &level);
    let handle = restored.handle_of("quest").ok_or("Missing quest")?;
    let count = |world: &EntityWorld| {
        world
            .components()
            .states
            .get(handle)
            .and_then(|bag| bag.get("count"))
            .and_then(crate::engine::entities::components::StateValue::as_int)
    };
    assert_eq!(
        count(&restored),
        Some(1),
        "startup actions must not run again after resume"
    );
    let _next = restored.dispatch_on(
        "quest",
        &[ActionDef::RunScript {
            script: path,
            handler: "entered".into(),
        }],
    );
    assert_eq!(
        count(&restored),
        Some(2),
        "closure state must continue from its saved value"
    );
    let mut malformed = serde_json::to_value(snapshot).map_err(|problem| problem.to_string())?;
    drop(
        malformed
            .get_mut("entities")
            .and_then(serde_json::Value::as_array_mut)
            .and_then(|records| records.first_mut())
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("Missing entity")?
            .insert("state".into(), serde_json::Value::Null),
    );
    let candidate = serde_json::from_value(malformed).map_err(|problem| problem.to_string())?;
    let mut refused = EntityWorld::from_level(&level);
    assert!(
        refused.restore_checkpoint(&candidate).is_err(),
        "a missing component cannot silently reset saved progression"
    );
    Ok(())
}

fn tick(world: &mut EntityWorld, level: &LevelDef) {
    let floor = WalkableFloor::from_level(level);
    let index = CollisionIndex::build(&[]);
    let _tick = world.tick(&WorldContext {
        delta_seconds: 0.016,
        feet_from: Vec3::new(2.0, 0.0, 2.0),
        feet: Vec3::new(2.0, 0.0, 2.0),
        eye: Vec3::new(2.0, 1.6, 2.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    });
    let _commands = world.take_commands();
}

#[test]
fn checkpoint_resolves_held_item_by_authored_id_in_a_fresh_solver() -> Result<(), String> {
    let mut original = sphere(7)?;
    assert!(
        original.pickup(7, Vec3::new(0.0, 1.5, 0.0), Vec3::NEG_Z, 0.0),
        "sphere is reachable"
    );
    let snapshot = original.checkpoint(&[(7, "item".into(), "fixture".into())])?;
    let mut restored = sphere(93)?;
    restored.restore_checkpoint(&[(93, "item".into(), "fixture".into())], &snapshot)?;
    assert_eq!(
        restored.held(),
        Some(93),
        "new runtime key is resolved from the authored item identity"
    );
    let before = original
        .snapshots()
        .into_iter()
        .next()
        .ok_or("Missing original")?;
    let after = restored
        .snapshots()
        .into_iter()
        .next()
        .ok_or("Missing restored")?;
    assert_eq!(
        before.pose, after.pose,
        "held object does not teleport during reconstruction"
    );
    Ok(())
}

fn sphere(key: u64) -> Result<ItemWorld, String> {
    let mut world = ItemWorld::default();
    world.spawn_authored(
        key,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            shape: Some(ShapeDef::Sphere { radius: 0.1 }),
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.1, 0.0, -0.1), Vec3::new(0.1, 0.2, 0.1)),
        Vec3::new(0.0, 1.4, -1.0),
        0.0,
        1.0,
    )?;
    Ok(world)
}

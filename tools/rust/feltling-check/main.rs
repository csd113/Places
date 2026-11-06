//! Explicit native CPU importer/controller/pose/lifecycle replay. No SDL,
//! audio device, GPU, map baking or ordinary-test dependency. Runtime resources
//! are selected explicitly through the same `PLACES_ASSET_ROOT` as production.
macro_rules! check {
    ($condition:expr, $message:literal $(,)?) => {
        if !$condition {
            return Err($message.into());
        }
    };
}
macro_rules! check_eq {
    ($left:expr, $right:expr, $message:literal $(,)?) => {
        if $left != $right {
            return Err($message.into());
        }
    };
}

fn main() -> Result<(), String> {
    use std::io::Write;
    let mut args = std::env::args_os().skip(1);
    let output = std::path::PathBuf::from(
        args.next()
            .ok_or("Usage: places-feltling-check <new-report.json>; set PLACES_ASSET_ROOT")?,
    );
    if args.next().is_some() || output.exists() {
        return Err("Use one new report path".into());
    }
    let result = lifecycle().and_then(|()| terrain_lifecycle());
    let report = serde_json::json!({"passed":result.is_ok(),"error":result.as_ref().err(),
        "scope":"Native CPU engine import, blended pose continuation, completion binding, derived status save/load, disable audio ownership and fixed-membership despawn. Includes rendered-tread pose continuation. No GPU, device playback or human visual/listening acceptance.",
        "model_sha256":"f89602b39812c5112007e324ee358bdef523a8ba9bc697d2636824aea4ee1d7b"});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    result
}

// This tiny CPU fixture adds the actual importer/controller/event/checkpoint
// bridge absent from the pure numerical policy checks. It decodes one 925 KB
// model and advances two seconds of one actor; no GPU, device or map bake.
fn lifecycle() -> Result<(), String> {
    use glam::Vec3;
    use places::engine::collision_index::CollisionIndex;
    use places::engine::entities::{EntityWorld, WorldCommand, WorldContext};
    use places::engine::level::{LevelDef, WalkableFloor};
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"feltling_cpu","name":"Feltling CPU","spawn":{"x":2,"z":2},"rooms":[{"x":-3,"z":-3,"width":6,"depth":6,"height":3}],"props":[{"id":"f","model":"feltling","solid":false,"feltling":{"mode":"talk"},"components":[{"component":"state","name":"completed","value":false},{"component":"interactable","prompt":"Talk"}],"bindings":[{"on":"animation_complete","key":"Pointing","actions":[{"action":"set_state","name":"completed","value":true}]}]}]}"#).map_err(|error|error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let ctx = WorldContext {
        delta_seconds: 0.1,
        player_walk_speed: 3.75,
        feet_from: Vec3::new(2.0, 0.0, 2.0),
        feet: Vec3::new(2.0, 0.0, 2.0),
        eye: Vec3::new(2.0, 1.6, 2.0),
        body_height: 1.8,
        body_half: 0.2,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let mut original = EntityWorld::from_level(&level);
    original.validate_guest_capability()?;
    let _initial_tick = original.tick(&ctx);
    let commands = original.take_commands();
    let source = commands
        .iter()
        .find_map(|command| {
            if let WorldCommand::PlaySound {
                entity,
                sound: _,
                gain: _,
                looped: _,
                time_domain: _,
            } = command
            {
                Some(*entity)
            } else {
                None
            }
        })
        .ok_or("Missing original voice command")?;
    check!(
        original.audio_source_anchor(source).is_some(),
        "live finite voice must retain its owning actor"
    );
    interrupt_gestures(&mut original, &ctx);
    let _commands = original.take_commands();
    let snapshot = original.checkpoint()?;
    let serialized = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    let decoded = serde_json::from_slice(&serialized).map_err(|error| error.to_string())?;
    let mut restored = EntityWorld::from_level(&level);
    restored.restore_checkpoint(&decoded)?;
    restored.validate_restored_guests(&ctx)?;
    check_eq!(
        original.entity_frames(),
        restored.entity_frames(),
        "resume must reconstruct the exact accepted blended pose without restarting it"
    );
    for _ in 0_u8..22 {
        let _resumed_tick = restored.tick(&ctx);
    }
    let handle = restored.handle_of("f").ok_or("Lost actor")?;
    check_eq!(
        restored
            .components()
            .states
            .get(handle)
            .and_then(|bag| bag.get("completed"))
            .and_then(places::engine::entities::components::StateValue::as_bool),
        Some(true),
        "one-shot completion must reach the ordinary authored binding once"
    );
    finish_lifecycle(restored, &level, &ctx, source)
}

fn interrupt_gestures(
    world: &mut places::engine::entities::EntityWorld,
    ctx: &places::engine::entities::WorldContext<'_>,
) {
    use places::engine::level::ActionDef;
    for (name, ticks) in [
        ("Pointing", 7_u8),
        ("Pointing", 1),
        ("Waving_Both_Arms", 1),
        ("Pointing", 1),
    ] {
        let _request = world.dispatch_on(
            "f",
            &[ActionDef::PlayAnimation {
                target: None,
                clip: Some(name.into()),
                looped: false,
            }],
        );
        for _ in 0_u8..ticks {
            let _tick = world.tick(ctx);
        }
    }
}

fn finish_lifecycle(
    mut restored: places::engine::entities::EntityWorld,
    level: &places::engine::level::LevelDef,
    ctx: &places::engine::entities::WorldContext<'_>,
    source: u64,
) -> Result<(), String> {
    use places::engine::entities::EntityWorld;
    use places::engine::level::ActionDef;
    let _pre_disable_commands = restored.take_commands();
    let _disabled = restored.dispatch_on("f", &[ActionDef::Disable { target: None }]);
    let _disabled_tick = restored.tick(ctx);
    let _post_disable_commands = restored.take_commands();
    check!(
        restored.audio_source_anchor(source).is_none(),
        "disabled ownership cannot revive a queued voice"
    );
    let disabled = restored.checkpoint()?;
    let mut disabled_restore = EntityWorld::from_level(level);
    disabled_restore.restore_checkpoint(&disabled)?;
    check!(
        disabled_restore
            .entity_frames()
            .iter()
            .all(|frame| frame.opacity <= 0.0),
        "disabled checkpoint must stay hidden"
    );
    let _enable = disabled_restore.dispatch_on("f", &[ActionDef::Enable { target: None }]);
    let occupied = places::engine::entities::WorldContext {
        feet: glam::Vec3::ZERO,
        feet_from: glam::Vec3::ZERO,
        eye: glam::Vec3::new(0.0, 1.6, 0.0),
        ..*ctx
    };
    let _occupied_tick = disabled_restore.tick(&occupied);
    check!(
        disabled_restore
            .entity_frames()
            .iter()
            .all(|frame| frame.opacity <= 0.0),
        "enable cannot materialize an actor inside the player"
    );
    let _clear_tick = disabled_restore.tick(ctx);
    check!(
        disabled_restore
            .entity_frames()
            .iter()
            .any(|frame| frame.opacity > 0.0),
        "pending enable must resume after the retained location is clear"
    );
    let _removed =
        disabled_restore.dispatch_on("f", &[ActionDef::DespawnEntity { target: "f".into() }]);
    check!(
        disabled_restore.handle_of("f").is_none(),
        "despawn must release creature and ordinary entity ownership"
    );
    check!(
        disabled_restore.checkpoint().is_err(),
        "fixed authored membership must still refuse a post-despawn save"
    );
    Ok(())
}

// The same actual importer/controller closure on an authored 150 mm stair,
// protecting nonzero terrain correction across saves without any map bake.
fn terrain_lifecycle() -> Result<(), String> {
    use glam::Vec3;
    use places::engine::collision_index::CollisionIndex;
    use places::engine::entities::{EntityWorld, WorldContext};
    use places::engine::level::{LevelDef, WalkableFloor};
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"feltling_stair_cpu","name":"Tread restore","spawn":{"x":2,"z":-1},"rooms":[{"x":-3,"z":-3,"width":6,"depth":8,"height":3}],"stairs":[{"x":-1,"z":0,"width":2,"depth":2.4,"rise":0.6,"steps":4}],"props":[{"id":"f","model":"feltling","x":0,"z":0.9,"solid":false,"feltling":{"mode":"talk"}}]}"#).map_err(|error|error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let ctx = WorldContext {
        delta_seconds: 0.1,
        player_walk_speed: 3.75,
        feet_from: Vec3::new(2.0, 0.0, -1.0),
        feet: Vec3::new(2.0, 0.0, -1.0),
        eye: Vec3::new(2.0, 1.6, -1.0),
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let mut original = EntityWorld::from_level(&level);
    original.validate_guest_capability()?;
    let _tick = original.tick(&ctx);
    let _commands = original.take_commands();
    let snapshot = original.checkpoint()?;
    let json = serde_json::to_value(&snapshot).map_err(|error| error.to_string())?;
    let correction = json
        .pointer("/feltlings/0/terrain_offsets/0")
        .and_then(serde_json::Value::as_f64)
        .ok_or("Missing terrain continuation")?;
    check!(
        (-0.10_f64..-0.05_f64).contains(&correction),
        "rendered tread support must differ from the continuous pitch line"
    );
    let mut restored = EntityWorld::from_level(&level);
    restored.restore_checkpoint(&snapshot)?;
    restored.validate_restored_guests(&ctx)?;
    check_eq!(
        original.entity_frames(),
        restored.entity_frames(),
        "stair resume must restore the exact corrected pose"
    );
    Ok(())
}

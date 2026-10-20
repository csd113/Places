//! The real Intake script runs against three tiny metadata props and a door.
//! Repeated recovery must request only the essential group, preserve completion,
//! and never repeat the reward. This costs a few bounded Luau calls; no map bake.

use crate::engine::entities::{EntityWorld, WorldCommand};
use crate::engine::level::{ActionDef, LevelDef};
use crate::engine::project::ProjectPath;
use crate::engine::scripting::ScriptSource;

#[test]
fn essential_supply_recovery_preserves_progress_and_other_items() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"recovery","name":"Recovery","spawn":{"x":0,"z":0},
      "props":[{"id":"commission","model":"core:crate","components":[{"component":"state","name":"stamp","value":false}]},
      {"id":"supply","model":"core:crate","physics":{"motion":"dynamic","reset_group":"essential_supply"}},
      {"id":"other","model":"core:crate","physics":{"motion":"dynamic","reset_group":"supplies"}}],
      "doors":[{"id":"airlock","x":0,"z":0,"width":1,"height":2,"locked":true}],
      "sound_emitters":[{"id":"ack","x":0,"y":0,"z":0,"sound":"test.ack"}]}"#).map_err(|error| error.to_string())?;
    let path = ProjectPath::new("scripts/relay_intake.luau".into())?;
    level.script_sources.push(ScriptSource {
        path: path.clone(),
        source: include_str!("../../../projects/relay-annex/scripts/relay_intake.luau").into(),
    });
    let mut world = EntityWorld::from_level(&level);
    let _initial = world.take_commands();
    for _ in 0_u8..2 {
        let report = world.dispatch_on(
            "commission",
            &[ActionDef::RunScript {
                script: path.clone(),
                handler: "well".into(),
            }],
        );
        let commands = world.take_commands();
        assert_eq!(commands.iter().filter(|command| matches!(command, WorldCommand::ResetPhysicsGroup {group} if group == "essential_supply")).count(),1,"each retry requests exactly the essential body group");
        assert!(!commands.iter().any(|command| matches!(command,WorldCommand::ResetPhysicsGroup {group} if group != "essential_supply")),"unrelated bodies cannot reset");
        assert!(
            !report.player_reset,
            "recovery must preserve the current visit"
        );
    }
    let _delivered = world.dispatch_on(
        "commission",
        &[ActionDef::RunScript {
            script: path.clone(),
            handler: "returned".into(),
        }],
    );
    assert_eq!(world.take_commands().iter().filter(|command| matches!(command,WorldCommand::PlaySound {sound,entity:_,gain:_,looped:_,time_domain:_} if sound == "test.ack")).count(),1,"delivery rewards once");
    for handler in ["well", "returned"] {
        let _report = world.dispatch_on(
            "commission",
            &[ActionDef::RunScript {
                script: path.clone(),
                handler: handler.into(),
            }],
        );
        assert!(
            !world.take_commands().iter().any(|command| matches!(
                command,
                WorldCommand::ResetPhysicsGroup { group: _ }
                    | WorldCommand::PlaySound {
                        entity: _,
                        sound: _,
                        gain: _,
                        looped: _,
                        time_domain: _
                    }
            )),
            "accepted supply cannot reset or reward again"
        );
    }
    let stamp = world
        .handle_of("commission")
        .and_then(|handle| world.components().states.get(handle))
        .and_then(|state| state.get("stamp"))
        .and_then(crate::engine::entities::components::StateValue::as_bool);
    assert_eq!(
        stamp,
        Some(true),
        "earned progress survives recovery attempts"
    );
    Ok(())
}

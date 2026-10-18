//! Essential sandbox/lifecycle regressions: tiny metadata-only worlds exercise
//! actual Luau and the existing dispatcher, without assets, GPU, audio or baking.
//! They uniquely cover hostile VM code, atomic effect refusal, scoped capabilities
//! and replay isolation; intentional exhaustion costs only milliseconds per case.

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::events::EventKind;
use crate::engine::entities::{EntityWorld, WorldCommand, WorldContext};
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};
use crate::engine::project::ProjectPath;
use crate::engine::scripting::{ScriptSource, check_level};
use glam::Vec3;

fn fixture(source: &str) -> Result<LevelDef, String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"luau_fixture","name":"Luau fixture","spawn":{"x":3,"z":3},
      "rooms":[{"width":4,"depth":4}],
      "props":[{"id":"item","model":"core:rubber_duck","x":1,"z":1,"components":[{"component":"state","name":"count","value":0}]},{"id":"physical","model":"core:rubber_duck","x":1.5,"z":1,"physics":{"motion":"dynamic","reset_group":"items"}}],
      "doors":[{"id":"door","x":0,"z":0,"width":1,"height":2,"bidirectional":true}],
      "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","id":"lamp","x":2,"z":2,"switchable":true}],
      "sound_emitters":[{"id":"sound","x":1,"y":1,"z":1,"sound":"test.cue","looped":true,"playing":false}],
      "spawn_templates":[{"id":"template","model":"core:rubber_duck","lifetime_seconds":1}],
      "spawn_points":[{"id":"point","x":2,"z":2,"template":"template"}],
      "volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bindings":[{"on":"enter_volume","actions":[{"action":"run_script","script":"scripts/test.luau","handler":"entered"}]}]}]}"#).map_err(|e|e.to_string())?;
    crate::engine::loader::validate_prop_physics(&level)?;
    level.script_sources.push(ScriptSource {
        path: path()?,
        source: source.into(),
    });
    Ok(level)
}
fn path() -> Result<ProjectPath, String> {
    ProjectPath::new("scripts/test.luau".into())
}
fn invoke(world: &mut EntityWorld) -> Result<crate::engine::entities::DispatchReport, String> {
    Ok(world.dispatch_on(
        "zone",
        &[ActionDef::RunScript {
            script: path()?,
            handler: "entered".into(),
        }],
    ))
}
fn tick(world: &mut EntityWorld, level: &LevelDef) {
    let index = CollisionIndex::build(&[]);
    let floor = WalkableFloor::from_level(level);
    let _tick = world.tick(&WorldContext {
        delta_seconds: 0.016,
        feet_from: Vec3::splat(3.0),
        feet: Vec3::splat(3.0),
        eye: Vec3::splat(3.0),
        body_height: 1.8,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    });
}
fn count(world: &EntityWorld) -> Result<i64, String> {
    world
        .handle_of("item")
        .and_then(|h| world.components().states.get(h))
        .and_then(|s| s.get("count"))
        .and_then(crate::engine::entities::components::StateValue::as_int)
        .ok_or_else(|| "Missing state".into())
}

#[test]
fn script_trigger_uses_existing_actions_and_event_context() -> Result<(), String> {
    let level = fixture(
        r#"--!strict
        local n: number = 0
        return {entered=function(api,event)
          assert(event.kind == "enter_volume" and event.subject == "zone" and event.key == "arrival")
          n += 1
          api.door("door","open"); api.light("lamp",false); api.sound("sound",true)
          api.physics("items",true); api.spawn("point"); api.set_state("item","count",n)
        end}"#,
    )?;
    check_level(&level)?;
    let mut world = EntityWorld::from_level(&level);
    let _initial = world.take_commands();
    let zone = world.handle_of("zone").ok_or("Missing volume")?;
    assert!(
        world.emit(EventKind::EnterVolume, zone, "arrival", None),
        "event queued"
    );
    tick(&mut world, &level);
    assert_eq!(
        count(&world)?,
        1,
        "script state reached shared state dispatcher"
    );
    assert_eq!(
        world.live_spawns().len(),
        1,
        "shared spawner creates one instance"
    );
    assert!(
        world
            .doors()
            .get(0)
            .is_some_and(|door| door.phase() != crate::engine::door::DoorPhase::Closed),
        "script requested shared door motion"
    );
    let commands = world.take_commands();
    assert!(
        commands
            .iter()
            .any(|v| matches!(v,WorldCommand::ActivatePhysicsGroup{group} if group=="items")),
        "shared physics activation"
    );
    assert!(
        commands.iter().any(|v| matches!(
            v,
            WorldCommand::PlaySound {
                looped: true,
                entity: _,
                sound: _,
                gain: _,
                time_domain: _
            }
        )),
        "authored sound policy reused"
    );
    assert!(
        world.script_diagnostics().is_empty(),
        "valid script has no errors"
    );
    Ok(())
}

#[test]
fn sandbox_and_resource_failures_publish_no_partial_effects() -> Result<(), String> {
    for body in [
        "while true do end",
        "local huge = string.rep('x', 4000000)",
        "for i=1,100 do api.spawn('point') end",
        "api.light('lamp', 1)",
        "api.door('missing','open')",
        "api.set_state('item','new_slot',true)",
        "api.set_state('item','count',0/0)",
        "api.set_state('item','count','wrong')",
        "os.execute('touch forbidden')",
        "error(setmetatable({}, {__tostring=function() while true do end end}))",
        "require('elsewhere')",
        "error(string.rep('e', 2000))",
    ] {
        let level = fixture(&format!(
            "return {{entered=function(api,event) api.door('door','open'); {body} end}}"
        ))?;
        let mut world = EntityWorld::from_level(&level);
        let _initial = world.take_commands();
        let report = invoke(&mut world)?;
        assert_eq!(
            report.actions_run, 0,
            "failed handler discards all staged actions: {body}"
        );
        assert!(
            world.take_commands().is_empty(),
            "failed handler emits no commands: {body}"
        );
        assert_eq!(
            world.live_spawns().len(),
            0,
            "failed spawn burst stays atomic"
        );
        assert_eq!(
            world.script_diagnostics().len(),
            1,
            "one bounded diagnostic: {body}"
        );
        assert!(
            world
                .script_diagnostics()
                .first()
                .is_some_and(|s| s.chars().count() <= 512),
            "error length bounded"
        );
        let _retry = invoke(&mut world)?;
        assert_eq!(
            world.script_diagnostics().len(),
            1,
            "failed module stays disabled"
        );
    }
    let level = fixture(
        "assert(io==nil and os==nil and debug==nil and package==nil and require==nil and loadstring==nil and pcall==nil and xpcall==nil and coroutine==nil and string.find==nil and string.split==nil); return {entered=function(api) api.set_state('item','count',1) end}",
    )?;
    let mut world = EntityWorld::from_level(&level);
    let _report = invoke(&mut world)?;
    assert_eq!(
        count(&world)?,
        1,
        "safe standard library and strict sandbox remain usable"
    );
    Ok(())
}

#[test]
fn reset_reload_and_scoped_api_prevent_stale_state() -> Result<(), String> {
    let level = fixture(
        "local n = 0; return {entered=function(api) n+=1; api.set_state('item','count',n) end}",
    )?;
    let mut world = EntityWorld::from_level(&level);
    for _ in 0..12u8 {
        let _first = invoke(&mut world)?;
        let _second = invoke(&mut world)?;
        assert_eq!(count(&world)?, 2, "closure persists only during its run");
        world.reset_runtime();
        assert_eq!(count(&world)?, 0, "authored state restored on reset");
    }
    let stale = fixture(
        "local old; return {entered=function(api) if old then old.door('door','open') else old=api end end}",
    )?;
    for _ in 0..8u8 {
        world.resolve(&stale);
        let _first = invoke(&mut world)?;
        let second = invoke(&mut world)?;
        assert_eq!(
            second.actions_run, 0,
            "retained scope cannot use a prior Rust world view"
        );
        assert_eq!(
            world.script_diagnostics().len(),
            1,
            "retained host API fails usefully"
        );
    }
    world.resolve(&fixture(
        "return {entered=function(api) api.set_state('item','count',7) end}",
    )?);
    let _final = invoke(&mut world)?;
    assert_eq!(
        count(&world)?,
        7,
        "new map cannot invoke replaced module/state"
    );
    assert!(
        world.script_diagnostics().is_empty(),
        "new map has fresh diagnostics"
    );
    Ok(())
}

#[test]
fn source_and_handler_contracts_fail_before_gameplay() -> Result<(), String> {
    for source in [
        "return 3",
        "return {entered=1}",
        "return {entered=function(",
        "while true do end",
        "return setmetatable({}, {__index=function() while true do end end})",
    ] {
        assert!(
            check_level(&fixture(source)?).is_err(),
            "invalid or missing export refused: {source}"
        );
    }
    assert!(
        crate::engine::scripting::handlers(&ScriptSource {
            path: path()?,
            source: "return {function() end}".into()
        })
        .is_err(),
        "numeric array keys cannot masquerade as named exports"
    );
    let valid = fixture("return {entered=function() end}")?;
    assert!(
        crate::engine::scripting::validate_package_sources(&valid, &[]).is_err(),
        "scripted packages must negotiate Luau support"
    );
    crate::engine::scripting::validate_package_sources(&valid, &["luau-v1".into()])?;
    let mut missing = fixture("return {different=function() end}")?;
    assert!(
        check_level(&missing).is_err(),
        "missing linked handler refused"
    );
    missing.script_sources.clear();
    assert!(
        crate::engine::scripting::validate_compiled_sources(&missing).is_err(),
        "package must embed linked source"
    );
    let oversized = fixture(&"x".repeat(65_537))?;
    assert!(
        check_level(&oversized).is_err(),
        "source byte cap before VM creation"
    );
    Ok(())
}

#[test]
fn automatic_handlers_replay_without_retaining_per_call_host_allocations() -> Result<(), String> {
    let level = fixture(
        "local n=0; return {start=function(api) n=10 end,update=function(api,event) assert(event.dt>0); n+=1; api.set_state('item','count',n) end,entered=function() end}",
    )?;
    let mut world = EntityWorld::from_level(&level);
    for _ in 0..256u16 {
        tick(&mut world, &level);
    }
    assert_eq!(
        count(&world)?,
        266,
        "start once and update once per playing tick"
    );
    assert!(
        world.script_diagnostics().is_empty(),
        "scoped API allocations remain within the VM cap across repeated calls"
    );
    world.reset_runtime();
    tick(&mut world, &level);
    assert_eq!(
        count(&world)?,
        11,
        "reset recreates automatic handler closure state"
    );
    Ok(())
}

#[test]
fn repeated_script_calls_share_a_frame_action_budget() -> Result<(), String> {
    let level = fixture(
        "return {entered=function(api) for n=1,32 do api.set_state('item','count',n) end end}",
    )?;
    let mut world = EntityWorld::from_level(&level);
    let mut total = 0usize;
    for _ in 0..16u8 {
        total = total.saturating_add(invoke(&mut world)?.actions_run);
    }
    assert!(
        total <= 128,
        "separate batches cannot bypass the per-frame action bound"
    );
    assert_eq!(
        world.script_diagnostics().len(),
        1,
        "frame exhaustion disables only the offending module"
    );
    world.reset_runtime();
    assert_eq!(
        invoke(&mut world)?.actions_run,
        32,
        "reset releases the previous frame budget"
    );
    Ok(())
}

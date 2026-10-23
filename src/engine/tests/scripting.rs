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

// This adds the project navigation boundary to the existing tiny scripting world.
// It protects declared-destination enforcement and reset cancellation; no bake or
// assets are needed, and only three bounded handler invocations are added.
#[test]
fn script_transitions_are_declared_atomic_and_cleared_by_reset() -> Result<(), String> {
    let mut level = fixture("return {entered=function(api) api.change_level('second') end}")?;
    level.transitions.push("second".into());
    check_level(&level)?;
    let mut world = EntityWorld::from_level(&level);
    let _initial = world.take_commands();
    let _first = invoke(&mut world)?;
    assert!(
        world.take_commands().iter().any(
            |command| matches!(command, WorldCommand::ChangeLevel { level: destination } if destination == "second")
        ),
        "script request uses the shared world-command queue"
    );
    let _pending = invoke(&mut world)?;
    world.reset_runtime();
    assert!(
        !world
            .take_commands()
            .iter()
            .any(|command| matches!(command, WorldCommand::ChangeLevel { level: _ })),
        "reset cancels pending navigation and re-arms the map"
    );
    level.script_sources.first_mut().ok_or("Missing source")?.source = "return {entered=function(api) api.change_level('second'); api.change_level('outside') end}".into();
    let mut invalid = EntityWorld::from_level(&level);
    let _initial_invalid = invalid.take_commands();
    let _refused = invoke(&mut invalid)?;
    assert!(
        !invalid
            .take_commands()
            .iter()
            .any(|command| matches!(command, WorldCommand::ChangeLevel { level: _ })),
        "an undeclared destination atomically discards earlier staged transition requests"
    );
    Ok(())
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
        "table.move({},1,10000000,-10000000)",
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

// Protect actual panel bounds, atomic rollback and pending-panel cancellation.
// One metadata-only world, four scoped Luau calls; no assets or bake.
#[test]
fn guidance_and_outcomes_are_bounded_atomic_and_cancelled_on_reset() -> Result<(), String> {
    let level = fixture(
        "return {entered=function(api) api.message('Goal', {'Bring gifts home'}); api.finish(true, 'Home', 'You made it.') end}",
    )?;
    let mut world = EntityWorld::from_level(&level);
    let _initial = world.take_commands();
    let _report = invoke(&mut world)?;
    let commands = world.take_commands();
    assert!(
        matches!(commands.first(), Some(WorldCommand::ShowMessage { title, lines }) if title == "Goal" && lines.len() == 1),
        "ordinary scoped calls publish bounded guidance"
    );
    assert!(
        matches!(
            commands.get(1),
            Some(WorldCommand::EndRun {
                won: true,
                title: _,
                detail: _
            })
        ),
        "outcome follows authored command order"
    );
    let _pending = invoke(&mut world)?;
    world.reset_runtime();
    assert!(
        !world.take_commands().iter().any(|command| matches!(
            command,
            WorldCommand::ShowMessage { title: _, lines: _ }
                | WorldCommand::EndRun {
                    won: _,
                    title: _,
                    detail: _
                }
        )),
        "a reset cannot reopen stale guidance/results"
    );
    let invalid = fixture(
        "return {entered=function(api) api.finish(true, 'Home', 'Done'); api.message('Goal', {[1]='valid',[1000]='hidden'}) end}",
    )?;
    let mut refused = EntityWorld::from_level(&invalid);
    let _initial_refused = refused.take_commands();
    let _rejected = invoke(&mut refused)?;
    assert!(
        !refused.take_commands().iter().any(|command| matches!(
            command,
            WorldCommand::EndRun {
                won: _,
                title: _,
                detail: _
            }
        )),
        "invalid sparse guidance rolls back prior staged outcome"
    );
    assert!(
        !refused.script_diagnostics().is_empty(),
        "rejection is visible in diagnostics"
    );
    Ok(())
}

// Protect the visible-only contract independently from the AI release grace.
// A single metadata actor and ten direct light samples cost no GPU/bake work.
#[test]
fn beam_reveal_clears_before_release_grace_and_restores_on_reset() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"beam_fixture","name":"Beam fixture","spawn":{"x":3,"z":3},"rooms":[{"width":4,"depth":4}],"props":[{"id":"actor","model":"sheet-ghost","x":1,"z":1,"components":[{"component":"nav_agent","radius":0.3,"height":1.6,"speed_mps":1},{"component":"ai","behavior":"idler"},{"component":"light_response","reveal_only":true,"release_seconds":0.5}]}]}"#).map_err(|e|e.to_string())?;
    level.script_sources.push(ScriptSource {
        path: path()?,
        source: "return {start=function(api) local a=api.actor('actor'); assert(a.x==1 and a.y==0 and a.z==1, 'stale reset host pose') end}".into(),
    });
    let mut world = EntityWorld::from_level(&level);
    assert!(
        world
            .entity_frames()
            .first()
            .is_some_and(|frame| frame.opacity.abs() <= f32::EPSILON),
        "unilluminated actor starts hidden"
    );
    world.update_light_observations(0.016, Vec3::splat(3.0), |_, _| 1.0);
    assert!(
        world
            .entity_frames()
            .first()
            .is_some_and(|frame| frame.opacity > 0.99),
        "direct beam reveals the actor"
    );
    world.update_light_observations(0.1, Vec3::splat(3.0), |_, _| 0.0);
    assert!(
        world
            .light_response_observations()
            .any(|response| response.active),
        "locomotion release grace still protects the player"
    );
    assert!(
        world
            .entity_frames()
            .first()
            .is_some_and(|frame| frame.opacity.abs() <= f32::EPSILON),
        "release grace never leaves a visible ghost outside the beam"
    );
    let actor = world.handle_of("actor").ok_or("Missing actor")?;
    world
        .components_mut()
        .transforms
        .get_mut(actor)
        .ok_or("Missing transform")?
        .position = Vec3::new(2.0, 0.0, 2.0);
    world.reset_runtime();
    let authored = Vec3::new(1.0, 0.0, 1.0);
    assert!(
        world
            .ai()
            .agents()
            .first()
            .is_some_and(|agent| agent.position == authored)
            && world
                .entity_frames()
                .first()
                .and_then(|frame| frame.transform)
                .is_some_and(|(position, _)| position == authored),
        "AI and renderer-bound frame agree immediately before movement"
    );
    assert!(
        world.light_response_observations().all(|response| response
            .position
            .distance_squared(Vec3::new(1.0, 0.0, 1.0))
            <= f32::EPSILON),
        "idle reset restores script and beam anchors before another AI movement handoff"
    );

    assert!(
        world
            .light_response_observations()
            .all(|response| !response.active && response.intensity.abs() <= f32::EPSILON),
        "restart clears renderer observations and hold state"
    );
    let mut reset_samples = Vec::new();
    world.update_light_observations(0.016, Vec3::splat(3.0), |_, sample| {
        reset_samples.push(sample);
        1.0
    });
    assert!(
        reset_samples
            .first()
            .is_some_and(|sample| (sample.x, sample.z) == (authored.x, authored.z))
            && reset_samples
                .iter()
                .all(|sample| (sample.x - authored.x).abs() <= 0.3
                    && (sample.z - authored.z).abs() <= 0.3
                    && sample.y > 0.0
                    && sample.y < 1.6),
        "first new beam samples the torso and partial-body anchors around the reset body"
    );
    tick(&mut world, &level);
    assert!(
        world.script_diagnostics().is_empty(),
        "actual scoped Luau reads the reset position before movement"
    );
    Ok(())
}

// Protect the exact host observation boundary used by scripted contact rules:
// one thin wall blocks sight while a goal still uses the normal AI dispatcher.
// Two metadata ticks, one actor, no navigation bake or external resources.
#[test]
fn script_actor_sight_uses_collision_and_navigation_keeps_its_goal() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"sight_fixture","name":"Sight fixture","spawn":{"x":3,"z":1},"rooms":[{"width":4,"depth":4}],"props":[{"id":"actor","model":"sheet-ghost","x":1,"z":1,"components":[{"component":"nav_agent","radius":0.3,"height":1.6,"speed_mps":1},{"component":"ai","behavior":"idler"},{"component":"state","name":"clear","value":true}]}]}"#).map_err(|e|e.to_string())?;
    level.script_sources.push(ScriptSource { path: path()?, source: "return {update=function(api) local a=api.actor('actor'); local p=api.player(); api.set_state('actor','clear',a.player_visible); api.navigate('actor',p.x,p.y,p.z) end}".into() });
    let mut world = EntityWorld::from_level(&level);
    let walls = [crate::engine::collision::WallAabb::with_y(
        1.99, 0.0, 0.0, 0.02, 2.5, 4.0,
    )];
    let index = CollisionIndex::build(&walls);
    let floor = WalkableFloor::from_level(&level);
    let mut context = WorldContext {
        delta_seconds: 0.016,
        feet_from: Vec3::new(3.0, 0.0, 1.0),
        feet: Vec3::new(3.0, 0.0, 1.0),
        eye: Vec3::new(3.0, 1.6, 1.0),
        body_height: 1.8,
        walls: &walls,
        index: &index,
        floor: &floor,
        nav: None,
    };
    let _blocked = world.tick(&context);
    let handle = world.handle_of("actor").ok_or("Missing actor")?;
    assert!(
        world
            .components()
            .states
            .get(handle)
            .and_then(|state| state.get("clear"))
            .is_some_and(|value| matches!(
                value,
                crate::engine::entities::components::StateValue::Bool(false)
            )),
        "a thin physical wall blocks script contact visibility"
    );
    assert!(
        world
            .ai()
            .agents()
            .first()
            .is_some_and(|actor| actor.navigation_goal == Some(context.feet)),
        "script queues a goal rather than teleporting the actor"
    );
    let clear_index = CollisionIndex::build(&[]);
    context.walls = &[];
    context.index = &clear_index;
    let _clear = world.tick(&context);
    assert!(
        world
            .components()
            .states
            .get(handle)
            .and_then(|state| state.get("clear"))
            .is_some_and(|value| matches!(
                value,
                crate::engine::entities::components::StateValue::Bool(true)
            )),
        "removing the wall updates the next invocation's observation"
    );
    // Extend this one-actor fixture across door changes and reset: batched
    // leaves must retain pre-script timing and never survive into the next tick.
    level.doors.push(
        serde_json::from_str(
            r#"{"id":"leaf","x":2,"z":1.25,"rotation_degrees":90,"width":0.5,"height":2.5}"#,
        )
        .map_err(|problem| problem.to_string())?,
    );
    let mut door_world = EntityWorld::from_level(&level);
    for open in [false, true, false] {
        if open {
            let _opened = door_world.dispatch_on("leaf", &[ActionDef::Open { target: None }]);
            context.delta_seconds = 0.25;
            for _ in 0..8_u8 {
                let _moved = door_world.update_doors(&context);
            }
        } else {
            door_world.reset_runtime();
        }
        context.delta_seconds = 0.016;
        let _observed = door_world.tick(&context);
        assert!(
            door_world
                .handle_of("actor")
                .and_then(|actor| door_world.components().states.get(actor))
                .and_then(|state| state.get("clear"))
                .is_some_and(|value| matches!(value, crate::engine::entities::components::StateValue::Bool(visible) if *visible == open)),
            "script sight follows the current hinged leaf after open/reset: open={open}, door={:?}, diagnostics={:?}", door_world.doors().get(0).map(|leaf| (leaf.angle(), leaf.phase())), door_world.script_diagnostics()
        );
    }
    Ok(())
}

// A two-entity metadata fixture protects the distinction between requested and
// settled door state, immutable observations and replay. It adds no bake/assets
// or native device and runs four bounded VM calls in milliseconds.
#[test]
fn observations_report_live_door_state_and_replay_atomically() -> Result<(), String> {
    let mut level = fixture(
        r"return {entered=function(api)
        local d=api.door_status('door')
        if d.phase ~= 'closed' or d.obstructed or d.angle ~= 0 then error('bad leaf') end
        local a=api.actor('item')
        if math.abs(a.radius-0.55)>0.000001 or math.abs(a.height-1.9)>0.000001 then error('bad body') end
        api.door('door','lock')
        api.prompt('item','Release latch')
        api.set_state('item','count', d.locked and 2 or 1)
    end}",
    )?;
    let item = level.props.first_mut().ok_or("Missing fixture item")?;
    for source in [
        r#"{"component":"nav_agent","radius":0.55,"height":1.9,"speed_mps":1}"#,
        r#"{"component":"interactable","prompt":"Latch"}"#,
    ] {
        item.components
            .push(serde_json::from_str(source).map_err(|problem| problem.to_string())?);
    }
    let mut world = EntityWorld::from_level(&level);
    let _initial = world.take_commands();
    let _first = invoke(&mut world)?;
    assert_eq!(
        count(&world)?,
        1,
        "query retains invocation-start state after a queued lock"
    );
    assert_eq!(
        world.doors().is_locked("door"),
        Some(true),
        "shared lock action is published"
    );
    let _second = invoke(&mut world)?;
    assert_eq!(count(&world)?, 2, "next invocation sees actual lock state");
    world.reset_runtime();
    let _replayed = invoke(&mut world)?;
    assert_eq!(
        count(&world)?,
        1,
        "replay restores the authored leaf before new queries"
    );
    assert!(
        world.script_diagnostics().is_empty(),
        "valid observation calls stay enabled"
    );
    level.script_sources.first_mut().ok_or("Missing source")?.source =
        "return {entered=function(api) api.door('door','lock'); local d=api.door_status('door'); d.locked=true end}".into();
    let mut refused = EntityWorld::from_level(&level);
    let _refusal = invoke(&mut refused)?;
    assert_eq!(
        refused.doors().is_locked("door"),
        Some(false),
        "mutating a read-only observation discards queued effects"
    );
    assert_eq!(
        refused.script_diagnostics().len(),
        1,
        "failure disables the module once"
    );
    Ok(())
}

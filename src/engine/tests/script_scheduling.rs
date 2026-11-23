//! Small world fixtures cover callback admission, terminal dispatch order and
//! lifecycle against the actual Luau and event paths. Action counts force a
//! deterministic budget cut without timing loops, rendering or a map bake.

use glam::Vec3;

use super::events::EventQueue;
use super::{EntityWorld, EventKind, WorldCommand, WorldContext, WorldTick};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};
use crate::engine::project::ProjectPath;
use crate::engine::scripting::ScriptSource;

fn fixture() -> Result<LevelDef, String> {
    let mut level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"script_scheduling","name":"Script scheduling",
        "spawn":{"x":3,"z":3},"rooms":[{"width":4,"depth":4}],
        "props":[
          {"id":"retired","model":"core:rubber_duck","x":2,"z":1,"bindings":[
            {"on":"interact","actions":[{"action":"show_message","title":"retired","lines":["Marker"]}]}]},
          {"id":"late","model":"core:rubber_duck","x":2,"z":2,
            "components":[{"component":"interactable"}],"bindings":[
            {"on":"interact","actions":[{"action":"show_message","title":"late","lines":["Marker"]}]}]},
          {"id":"item","model":"core:rubber_duck","x":1,"z":1,
            "components":[{"component":"state","name":"count","value":0}],
            "bindings":[
                {"on":"interact","key":"linked","actions":[
                    {"action":"run_script","script":"scripts/control.luau","handler":"linked"}]},
                {"on":"interact","key":"tail","actions":[
                    {"action":"show_message","title":"tail","lines":["Marker"]}]},
                {"on":"object_state","key":"count","actions":[
                    {"action":"show_message","title":"child","lines":["Marker"]}]},
                {"on":"sequence_complete","key":"done","cooldown_seconds":1.0,"actions":[
                    {"action":"show_message","title":"sequence prefix","lines":["Marker"]},
                    {"action":"run_script","script":"scripts/control.luau","handler":"sequence_end"},
                    {"action":"show_message","title":"sequence suffix","lines":["Marker"]}]}
            ]}],
        "sequences":[{"id":"done","steps":[{"step":"stop"}]}],
        "timers":[{"id":"once","seconds":0.125,"bindings":[{"on":"timer","once":true,"actions":[
            {"action":"show_message","title":"timer prefix","lines":["Marker"]},
            {"action":"run_script","script":"scripts/control.luau","handler":"timer_first"},
            {"action":"start_timer","seconds":0.125,"repeat":true},
            {"action":"stop_timer"},
            {"action":"show_message","title":"timer middle","lines":["Marker"]},
            {"action":"run_script","script":"scripts/control.luau","handler":"timer_second"},
            {"action":"show_message","title":"timer suffix","lines":["Marker"]}]}]}]
    }"#).map_err(|problem| problem.to_string())?;
    level.script_sources.push(ScriptSource {
        path: control()?,
        source: r#"
            local first_calls = 0
            return {
                burn=function(api)
                    for _ = 1,32 do api.set_state("item","count",0) end
                end,
                linked=function() end,
                linked_effect=function(api,event)
                    assert(event.kind == "object_state" and event.key == "count")
                    api.message("linked effect",{"Marker"})
                end,
                hint=function(api) api.message("fresh hint",{"Marker"}) end,
                timer_first=function(api,event)
                    assert(event.kind == "timer" and event.subject == "once" and event.key == "once")
                    first_calls += 1
                    for _ = 1,32 do api.set_state("item","count",1) end
                end,
                timer_second=function(api,event)
                    assert(event.kind == "timer" and event.subject == "once" and event.key == "once")
                    assert(first_calls == 1)
                    api.message("timer script",{"Marker"})
                end,
                sequence_end=function(api,event)
                    assert(event.kind == "sequence_complete" and event.subject == "item")
                    assert(event.key == "done" and event.actor == "item" and event.x == 1)
                    api.message("sequence script",{"Marker"})
                end,
            }
        "#.into(),
    });
    Ok(level)
}

fn control() -> Result<ProjectPath, String> {
    ProjectPath::new("scripts/control.luau".into())
}

fn spend_calls(world: &mut EntityWorld, count: usize) -> Result<(), String> {
    let action = ActionDef::RunScript {
        script: control()?,
        handler: "burn".into(),
    };
    for _ in 0..count {
        let report = world.dispatch_on("item", std::slice::from_ref(&action));
        assert_eq!(
            report.actions_run, 32,
            "fixture consumes one full call allowance"
        );
    }
    Ok(())
}

fn tick(world: &mut EntityWorld, level: &LevelDef, delta_seconds: f32) -> WorldTick {
    world.tick(&WorldContext {
        player_walk_speed: 3.75,
        delta_seconds,
        feet_from: Vec3::splat(3.0),
        feet: Vec3::splat(3.0),
        eye: Vec3::splat(3.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &CollisionIndex::build(&[]),
        floor: &WalkableFloor::from_level(level),
        nav: None,
    })
}

fn messages(world: &mut EntityWorld) -> Vec<(String, Vec<String>)> {
    world
        .take_commands()
        .into_iter()
        .filter_map(|command| {
            if let WorldCommand::ShowMessage { title, lines } = command {
                Some((title, lines))
            } else {
                None
            }
        })
        .collect()
}

fn titles(world: &mut EntityWorld) -> Vec<String> {
    messages(world)
        .into_iter()
        .map(|(title, _)| title)
        .collect()
}

fn fill_queue(world: &mut EntityWorld, key: &str) -> Result<(), String> {
    let item = world.handle_of("item").ok_or("Missing fixture entity")?;
    for _ in 0..world.events().remaining_capacity() {
        assert!(
            world.emit(EventKind::Interact, item, key, None),
            "available slot"
        );
    }
    Ok(())
}

#[test]
fn recurring_modules_rotate_under_budget_with_current_tick_delta() -> Result<(), String> {
    let mut level = fixture()?;
    for index in 0..3_u8 {
        level.script_sources.push(ScriptSource {
            path: ProjectPath::new(format!("scripts/auto_{index}.luau"))?,
            source: format!(
                r#"
                local starts = 0
                return {{
                    start = function() starts += 1 end,
                    update = function(api,event)
                        assert(starts == 1)
                        for _ = 1,7 do api.set_state("item","count",0) end
                        api.message("auto {index}",{{tostring(event.dt)}})
                    end,
                }}
            "#
            ),
        });
    }
    let mut world = EntityWorld::from_level(&level);
    let ticks: [(f32, &str, usize, &[&str]); 2] = [
        (0.125, "0.125", 3, &["auto 0"]),
        (0.25, "0.25", 2, &["auto 1", "auto 2", "auto 0"]),
    ];
    for (dt, expected_dt, prior_calls, order) in ticks {
        world.begin_script_frame();
        spend_calls(&mut world, prior_calls)?;
        // Every queued occurrence also links a script. It cannot consume the
        // automatic round or disable a module merely because the frame is full.
        fill_queue(&mut world, "linked")?;
        let _report = tick(&mut world, &level, dt);
        let actual = messages(&mut world);
        assert_eq!(
            actual
                .iter()
                .map(|(title, _)| title.as_str())
                .collect::<Vec<_>>(),
            order,
            "resume the first missed module and wrap once"
        );
        assert!(
            actual.iter().all(|(_, lines)| lines == &[expected_dt]),
            "resumed updates use the current tick, including deferred starts"
        );
        assert!(
            !world.script_failed(),
            "shared pressure must not disable modules"
        );
    }
    Ok(())
}

#[test]
fn admitted_timer_resumes_binding_and_keeps_its_synchronous_child_slot() -> Result<(), String> {
    let level = fixture()?;
    let mut world = EntityWorld::from_level(&level);
    let timer_handle = world.handle_of("once").ok_or("Missing fixture timer")?;
    let start_timer = ActionDef::StartTimer {
        target: None,
        seconds: None,
        repeat: None,
    };
    let _start = world.dispatch_on("once", std::slice::from_ref(&start_timer));
    world.begin_script_frame();
    spend_calls(&mut world, 3)?;
    world.tick_timers(0.125, &mut WorldTick::default());
    let item = world.handle_of("item").ok_or("Missing fixture entity")?;
    assert!(
        world.emit(EventKind::Interact, item, "tail", None),
        "tail marker fits"
    );
    fill_queue(&mut world, "fill")?;
    assert!(
        !world.emit(EventKind::Interact, item, "tail", None),
        "full FIFO refuses newest event"
    );
    let retired = world.handle_of("retired").ok_or("Missing fixture owner")?;
    assert!(
        world.remove_entity_handle(retired),
        "remove a preceding binding owner"
    );
    let _first = world.dispatch_queued_events();
    assert_eq!(
        titles(&mut world),
        ["timer prefix", "timer middle"],
        "only the executed prefix is published before yielding"
    );
    assert!(
        world
            .bindings
            .iter()
            .any(|binding| binding.owner == timer_handle && !binding.fired),
        "once is committed only after the final action"
    );
    assert!(
        !world.timers.get("once").is_some_and(|timer| timer.running),
        "the replacement timer was stopped by the running binding"
    );
    assert_eq!(
        world.events().len(),
        EventQueue::MAX_QUEUED_EVENTS,
        "the action's synchronous child gets the original event's freed slot"
    );
    assert_eq!(
        world.events().dropped(),
        1,
        "only the explicit overflow was dropped"
    );

    world.begin_script_frame();
    let _second = world.dispatch_queued_events();
    assert_eq!(
        titles(&mut world),
        ["timer script", "timer suffix", "tail"],
        "resume the first unexecuted action before later FIFO events"
    );
    for _ in 0..16_u8 {
        let _drain = world.dispatch_queued_events();
    }
    assert_eq!(
        titles(&mut world),
        ["child"],
        "synchronous child runs once after the admitted tail"
    );
    assert!(world.events().is_empty(), "all accepted events drained");

    let _rearm = world.dispatch_on("once", &[start_timer]);
    let _refire = tick(&mut world, &level, 0.125);
    assert!(
        titles(&mut world).is_empty(),
        "the completed once binding suppresses a later fire"
    );
    let _repeat = world.dispatch_on(
        "once",
        &[ActionDef::StartTimer {
            target: None,
            seconds: Some(0.125),
            repeat: Some(true),
        }],
    );
    let _long = tick(&mut world, &level, 1.0);
    let _half = tick(&mut world, &level, 0.0625);
    assert_eq!(
        world.timers.get("once").map(|timer| timer.fires),
        Some(3),
        "repeat resets to the whole period after one fire, even after a long tick"
    );
    let _due = tick(&mut world, &level, 0.0625);
    assert_eq!(
        world.timers.get("once").map(|timer| timer.fires),
        Some(4),
        "repeat fires again on the original schedule"
    );
    Ok(())
}

// The native interaction goes through its real enqueue-and-pump path with
// an expired budget. Only the held slot distinguishes it from a free FIFO slot.
#[test]
fn suspended_terminal_keeps_the_child_slot_against_a_later_interaction() -> Result<(), String> {
    let level = fixture()?;
    let mut world = EntityWorld::from_level(&level);
    let _start = world.dispatch_on(
        "once",
        &[ActionDef::StartTimer {
            target: None,
            seconds: None,
            repeat: None,
        }],
    );
    world.begin_script_frame();
    spend_calls(&mut world, 4)?;
    world.tick_timers(0.125, &mut WorldTick::default());
    let item = world.handle_of("item").ok_or("Missing fixture entity")?;
    assert!(
        world.emit(EventKind::Interact, item, "tail", None),
        "existing tail fits"
    );
    fill_queue(&mut world, "fill")?;
    let _first = world.dispatch_queued_events();
    assert_eq!(
        titles(&mut world),
        ["timer prefix"],
        "yield before any child"
    );
    assert_eq!(
        world.events().len(),
        EventQueue::MAX_QUEUED_EVENTS.saturating_sub(1),
        "only the terminal has left the FIFO"
    );
    let target = world.interactables.index_of("late");
    assert!(
        world
            .dispatch_interaction_from(target, Some(Vec3::ZERO))
            .is_some(),
        "later interaction reached the native event producer"
    );
    assert_eq!(
        world.events().dropped(),
        1,
        "the later interaction cannot take the child's slot"
    );

    world.begin_script_frame();
    let _second = world.dispatch_queued_events();
    assert_eq!(
        titles(&mut world),
        ["timer middle", "timer script", "timer suffix", "tail"],
        "existing events remain ahead of the child and the late event never runs"
    );
    for _ in 0..16_u8 {
        let _drain = world.dispatch_queued_events();
    }
    assert_eq!(
        titles(&mut world),
        ["child"],
        "the formerly fitting child is delivered once"
    );
    assert_eq!(
        world.events().dropped(),
        1,
        "the resumed child was admitted"
    );
    Ok(())
}

// This ordinary child binding uses real host actions and tests the residual
// budget, which the full-call guard previously confused with exhaustion.
#[test]
fn linked_child_uses_partial_budget_and_only_commits_once_when_run() -> Result<(), String> {
    let mut level = fixture()?;
    let child = level
        .props
        .iter_mut()
        .find(|prop| prop.id.as_deref() == Some("item"))
        .and_then(|prop| {
            prop.bindings
                .iter_mut()
                .find(|binding| binding.key.as_deref() == Some("count"))
        })
        .ok_or("Missing child binding")?;
    child.once = true;
    child.cooldown_seconds = 1.0;
    child.actions = vec![ActionDef::RunScript {
        script: control()?,
        handler: "linked_effect".into(),
    }];

    let mut world = EntityWorld::from_level(&level);
    let item = world.handle_of("item").ok_or("Missing fixture entity")?;
    world.begin_script_frame();
    spend_calls(&mut world, 4)?;
    assert!(
        world.emit(EventKind::ObjectState, item, "count", None),
        "first child is admitted"
    );
    let _exhausted = world.dispatch_queued_events();
    assert!(
        titles(&mut world).is_empty(),
        "fully spent frame cannot call the script"
    );
    assert!(
        world.bindings.iter().any(|binding| binding.owner == item
            && binding.once
            && !binding.fired
            && binding.cooldown_remaining <= 0.0),
        "a skipped call consumes neither once nor cooldown"
    );

    world.begin_script_frame();
    spend_calls(&mut world, 3)?;
    let _one = world.dispatch_on(
        "item",
        &[ActionDef::RunScript {
            script: control()?,
            handler: "hint".into(),
        }],
    );
    assert_eq!(
        titles(&mut world),
        ["fresh hint"],
        "fixture has spent 97 actions"
    );
    assert!(
        world.emit(EventKind::ObjectState, item, "count", None),
        "second child is admitted"
    );
    let _partial = world.dispatch_queued_events();
    assert_eq!(
        titles(&mut world),
        ["linked effect"],
        "one real host action fits the partial budget"
    );
    assert!(
        world.bindings.iter().any(|binding| binding.owner == item
            && binding.once
            && binding.fired
            && binding.cooldown_remaining > 0.0),
        "successful linked call commits both protections"
    );
    assert!(
        world.emit(EventKind::ObjectState, item, "count", None),
        "third child is admitted"
    );
    let _repeat = world.dispatch_queued_events();
    assert!(
        titles(&mut world).is_empty(),
        "the completed once binding cannot replay"
    );
    Ok(())
}

// A waiting step shares the VM with a recurring update. The second frame
// remains under enough pressure that the update would otherwise take its slot.
#[test]
fn sequence_link_waits_for_shared_budget_and_resumes_without_replay() -> Result<(), String> {
    let mut level = fixture()?;
    let source = level
        .script_sources
        .first_mut()
        .ok_or("Missing fixture script")?;
    source.source = r#"
        local steps = 0
        return {
            burn=function(api) for _=1,32 do api.set_state("item","count",0) end end,
            update=function(api) api.message("update",{"Marker"}) end,
            step=function(api,event)
                assert(event.subject == "item")
                steps += 1
                assert(steps == 1)
                api.message("step",{"Marker"})
            end,
        }
    "#
    .into();
    level.sequences.push(serde_json::from_str(r#"{"id":"script_step","steps":[
        {"step":"action","action":{"action":"show_message","title":"prefix","lines":["Marker"]}},
        {"step":"action","action":{"action":"run_script","script":"scripts/control.luau","handler":"step"}},
        {"step":"action","action":{"action":"show_message","title":"suffix","lines":["Marker"]}}
    ]}"#).map_err(|problem| problem.to_string())?);
    let mut world = EntityWorld::from_level(&level);
    assert!(
        world.start_sequence("item", "script_step"),
        "start the audited sequence"
    );
    world.begin_script_frame();
    spend_calls(&mut world, 4)?;
    let blocked = tick(&mut world, &level, 0.125);
    assert_eq!(
        blocked.sequences_completed, 0,
        "the sequence remains on the linked step"
    );
    assert_eq!(titles(&mut world), ["prefix"], "only the prefix executes");
    assert!(
        !world.script_failed(),
        "shared exhaustion does not disable the recurring module"
    );

    world.begin_script_frame();
    spend_calls(&mut world, 3)?;
    let resumed = tick(&mut world, &level, 0.125);
    assert_eq!(
        resumed.sequences_completed, 1,
        "the waiting step gets the next available call"
    );
    assert_eq!(
        titles(&mut world),
        ["step", "suffix"],
        "resume executes the ordered tail once"
    );
    let _later = tick(&mut world, &level, 0.125);
    assert_eq!(
        titles(&mut world),
        ["update"],
        "the healthy recurring callback still runs"
    );
    Ok(())
}

#[test]
fn admitted_sequence_resumes_then_commits_cooldown_and_reset_cancels_a_tail() -> Result<(), String>
{
    let level = fixture()?;
    let mut world = EntityWorld::from_level(&level);
    let sequence = ActionDef::StartSequence {
        target: None,
        sequence: "done".into(),
    };
    let run_sequence = |fixture_world: &mut EntityWorld| {
        let report = fixture_world.dispatch_on("item", std::slice::from_ref(&sequence));
        assert_eq!(report.sequences_started, 1, "fixture sequence starts");
    };
    run_sequence(&mut world);
    world.begin_script_frame();
    spend_calls(&mut world, 4)?;
    let first = tick(&mut world, &level, 0.125);
    assert_eq!(
        first.sequences_completed, 1,
        "the producer admits completion once"
    );
    assert_eq!(
        titles(&mut world),
        ["sequence prefix"],
        "binding has started"
    );
    assert!(
        !world.checkpoint_events_settled(),
        "in-flight bindings cannot enter a checkpoint"
    );
    let second = tick(&mut world, &level, 2.0);
    assert_eq!(
        second.sequences_completed, 0,
        "resume does not replay the sequence"
    );
    assert_eq!(
        titles(&mut world),
        ["sequence script", "sequence suffix"],
        "the terminal context and ordered tail survive the frame limit"
    );
    run_sequence(&mut world);
    let _cooldown = tick(&mut world, &level, 0.125);
    assert!(
        titles(&mut world).is_empty(),
        "cooldown begins when the terminal binding finishes"
    );

    run_sequence(&mut world);
    world.begin_script_frame();
    spend_calls(&mut world, 4)?;
    let _exhausted = tick(&mut world, &level, 2.0);
    assert_eq!(
        titles(&mut world),
        ["sequence prefix"],
        "second occurrence is suspended"
    );
    world.reset_runtime();
    let _next = tick(&mut world, &level, 0.125);
    assert!(
        titles(&mut world).is_empty(),
        "world reset discards the old continuation"
    );
    assert!(
        world.checkpoint_events_settled(),
        "reset leaves no pending dispatch"
    );
    Ok(())
}

#[test]
fn frame_input_uses_fresh_budget_and_shares_it_with_simulation() -> Result<(), String> {
    let level = fixture()?;
    let mut world = EntityWorld::from_level(&level);
    let _initial = tick(&mut world, &level, 0.125);
    spend_calls(&mut world, 4)?;
    world.begin_script_frame();
    assert!(
        !world.request_hint(),
        "the fixture hint does not reset the world"
    );
    assert_eq!(
        titles(&mut world),
        ["fresh hint"],
        "new frame does not charge the prior frame"
    );
    let _same_frame = tick(&mut world, &level, 0.125);
    assert_eq!(
        world
            .script_resources()
            .get("frame_actions")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "simulation uses the same budget already charged to input"
    );
    let _following_frame = tick(&mut world, &level, 0.125);
    assert!(titles(&mut world).is_empty(), "the press is not replayed");
    Ok(())
}

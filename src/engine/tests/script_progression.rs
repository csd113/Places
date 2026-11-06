//! Three focused SP-02/03/04 regressions on tiny metadata worlds. Actual Beam
//! modules retain retryable visits, reset rearms authored interaction capability,
//! and timer overrides expire on rearm. No assets, device, baking or wall sleep.

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::components::StateValue;
use crate::engine::entities::timers::Timers;
use crate::engine::entities::{EntityWorld, WorldCommand, WorldContext};
use crate::engine::level::WalkableFloor;
use crate::engine::level::{ActionDef, ConditionDef, LevelDef};
use crate::engine::project::ProjectPath;
use crate::engine::scripting::ScriptSource;
use glam::Vec3;

fn state(world: &EntityWorld, id: &str, name: &str) -> Option<StateValue> {
    world
        .handle_of(id)
        .and_then(|handle| world.components().states.get(handle))
        .and_then(|bag| bag.get(name).cloned())
}

fn settle(world: &mut EntityWorld, level: &LevelDef) {
    let floor = WalkableFloor::from_level(level);
    let index = CollisionIndex::build(&[]);
    let _tick = world.tick(&WorldContext {
        player_walk_speed: 3.75,
        delta_seconds: 0.016,
        feet_from: Vec3::ZERO,
        feet: Vec3::ZERO,
        eye: Vec3::Y,
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

fn call(
    world: &mut EntityWorld,
    path: &ProjectPath,
    handler: &str,
    subject: &str,
) -> Result<Vec<WorldCommand>, String> {
    let _report = world.dispatch_on(
        subject,
        &[ActionDef::RunScript {
            script: path.clone(),
            handler: handler.into(),
        }],
    );
    if world.script_failed() {
        return Err(format!("{:?}", world.script_diagnostics()));
    }
    Ok(world.take_commands())
}

fn beam_level(next: &str, previous: &str, receivers: &[&str]) -> Result<LevelDef, String> {
    let mut level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"beam_retry","name":"Beam retry","spawn":{"x":0,"z":0},
        "doors":[{"id":"exit","x":0,"z":0,"width":1,"height":2,"locked":true,"components":[{"component":"interactable"},{"component":"state","name":"won","value":false},{"component":"state","name":"count","value":0},{"component":"state","name":"ready","value":false},{"component":"state","name":"red_returned","value":false},{"component":"state","name":"green_returned","value":false},{"component":"state","name":"stars_returned","value":false},{"component":"state","name":"item_1_returned","value":false},{"component":"state","name":"item_2_returned","value":false},{"component":"state","name":"item_3_returned","value":false}]}],
        "sound_emitters":[{"id":"store_hum","x":0,"z":0,"sound":"test.hum","looped":true,"playing":true},{"id":"exit_cue","x":0,"z":0,"sound":"test.exit"},{"id":"returned_cue","x":0,"z":0,"sound":"test.return"}],
        "volumes":[{"id":"return_red","x":0,"z":0,"width":1,"depth":1},{"id":"return_green","x":0,"z":0,"width":1,"depth":1},{"id":"return_stars","x":0,"z":0,"width":1,"depth":1}]}"#).map_err(|problem| problem.to_string())?;
    level.volumes = receivers
        .iter()
        .map(|id| {
            serde_json::from_value(
                serde_json::json!({"id": id,"x":0_i32,"z":0_i32,"width":1_i32,"depth":1_i32}),
            )
            .map_err(|problem| problem.to_string())
        })
        .collect::<Result<_, _>>()?;
    level.transitions = [next, previous]
        .into_iter()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(level)
}

#[test]
fn beam_nonfinal_exits_remain_retryable_and_final_exit_requires_its_seal() -> Result<(), String> {
    for (source, next, previous, receivers) in [
        (
            include_str!("../../../projects/beam/scripts/beam_1.luau"),
            "beam_pool",
            "",
            ["return_red", "return_green", "return_stars"],
        ),
        (
            include_str!("../../../projects/beam/scripts/beam_pool_1.luau"),
            "beam_office",
            "beam",
            ["return_duck", "return_towel", "return_cleaner"],
        ),
        (
            include_str!("../../../projects/beam/scripts/beam_office_1.luau"),
            "beam_arcade",
            "beam_pool",
            ["return_records", "return_mug", "return_supplies"],
        ),
        (
            include_str!("../../../projects/beam/scripts/beam_arcade_1.luau"),
            "",
            "beam_office",
            ["return_plush", "return_gift", "return_ball"],
        ),
    ] {
        let mut level = beam_level(next, previous, &receivers)?;
        let path = ProjectPath::new("scripts/beam.luau".into())?;
        level.script_sources.push(ScriptSource {
            path: path.clone(),
            source: source.into(),
        });
        let mut world = EntityWorld::from_level(&level);
        let _initial = world.take_commands();
        let _started = call(&mut world, &path, "start", "exit")?;
        for receiver in receivers {
            let _returned = call(&mut world, &path, "returned", receiver)?;
        }
        for _ in 0_u8..3 {
            let commands = call(&mut world, &path, "exit", "exit")?;
            assert_eq!(commands.iter().filter(|command| matches!(command, WorldCommand::ChangeLevel { level: destination } if destination == next)).count(), usize::from(!next.is_empty()), "each live nonfinal visit can retry its destination");
            assert!(
                !commands.iter().any(|command| matches!(
                    command,
                    WorldCommand::EndRun {
                        won: _,
                        title: _,
                        detail: _
                    }
                )),
                "the final seal cannot be bypassed"
            );
            assert_eq!(
                state(&world, "exit", "won"),
                Some(StateValue::Bool(false)),
                "a request cannot commit terminal source state"
            );
            let hum = world
                .handle_of("store_hum")
                .and_then(|handle| world.components().audio.get(handle))
                .ok_or("missing hum")?;
            assert!(hum.playing, "rollback keeps the source ambience live");
        }
        let _removed = call(
            &mut world,
            &path,
            "removed",
            receivers.first().ok_or("missing receiver")?,
        )?;
        assert_eq!(
            state(&world, "exit", "count"),
            Some(StateValue::Int(2)),
            "item removal after a request revokes credit"
        );
        let blocked = call(&mut world, &path, "exit", "exit")?;
        assert!(
            !blocked
                .iter()
                .any(|command| matches!(command, WorldCommand::ChangeLevel { level: _ })),
            "missing items still block travel"
        );
        let hint = call(&mut world, &path, "hint", "exit")?;
        assert!(!hint.is_empty(), "guidance remains available");
        let back = call(&mut world, &path, "back", "exit")?;
        assert_eq!(back.iter().filter(|command| matches!(command, WorldCommand::ChangeLevel { level: destination } if destination == previous)).count(), usize::from(!previous.is_empty()), "authored back remains available");
        let _returned = call(
            &mut world,
            &path,
            "returned",
            receivers.first().ok_or("missing receiver")?,
        )?;
        let retry = call(&mut world, &path, "exit", "exit")?;
        assert_eq!(retry.iter().filter(|command| matches!(command, WorldCommand::ChangeLevel { level: destination } if destination == next)).count(), usize::from(!next.is_empty()), "recredit permits direct retry without reset");
    }
    Ok(())
}

fn assert_rearmed(world: &mut EntityWorld, index: usize, label: bool) -> Result<(), String> {
    let handle = world.handle_of("use").ok_or("missing use")?;
    let enabled = ConditionDef::Enabled {
        target: "use".into(),
    };
    let closed = ConditionDef::Disabled {
        target: "closed".into(),
    };
    world.reset_runtime();
    assert!(
        world.condition_holds(&enabled) && world.condition_holds(&closed),
        "reset restores both authored enabled defaults"
    );
    assert_eq!(
        state(world, "meter", "count"),
        Some(StateValue::Int(0)),
        "objective progress resets with capability"
    );
    assert_eq!(
        state(world, "use", "enabled"),
        Some(StateValue::Bool(true)),
        "synthetic enable state agrees with capability"
    );
    assert_eq!(
        state(world, "authored-state", "enabled"),
        Some(StateValue::Bool(false)),
        "explicit authored state is restored without being overwritten by synthetic state"
    );
    assert!(
        world.condition_holds(&ConditionDef::Enabled {
            target: "authored-state".into()
        }),
        "capability follows its own authored default"
    );
    let capability = world
        .components()
        .interactables
        .get(handle)
        .ok_or("missing restored capability")?;
    assert_eq!(capability.prompt, "Use", "authored prompt restores");
    assert_eq!(
        capability.label_visible, label,
        "existing label visibility policy is preserved"
    );
    assert!(
        world
            .interactables()
            .get(index)
            .is_some_and(|item| item.enabled),
        "aiming table must expose the restored action"
    );
    let _again = world
        .dispatch_interaction(Some(index))
        .ok_or("missing rearmed interaction")?;
    assert_eq!(
        state(world, "meter", "count"),
        Some(StateValue::Int(1)),
        "once binding rearms and action works after reset"
    );
    assert!(
        !world.condition_holds(&enabled),
        "rearmed action consumes normally"
    );
    Ok(())
}

// Two controls and an independent condition witness protect capability reset,
// authored-disabled policy and checkpoint continuation beyond closure resets.
#[test]
fn reset_restores_interaction_enablement_without_changing_label_policy() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"reset_interactions","name":"Reset interactions","spawn":{"x":0,"z":0},
        "props":[{"id":"use","model":"core:chair","components":[{"component":"interactable","prompt":"Use","label":"Original"}],"bindings":[{"on":"interact","once":true,"actions":[{"action":"set_state","target":"meter","name":"count","value":1},{"action":"disable"}]}]},
        {"id":"closed","model":"core:chair","components":[{"component":"interactable","enabled":false,"prompt":"Closed"}]},
        {"id":"meter","model":"core:chair","components":[{"component":"state","name":"count","value":0}]},
        {"id":"authored-state","model":"core:chair","components":[{"component":"interactable"},{"component":"state","name":"enabled","value":false}]}]}"#).map_err(|problem| problem.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let enabled = ConditionDef::Enabled {
        target: "use".into(),
    };
    let closed = ConditionDef::Disabled {
        target: "closed".into(),
    };
    assert!(world.condition_holds(&enabled) && world.condition_holds(&closed));
    let index = world
        .interactables()
        .index_of("use")
        .ok_or("missing control")?;
    let _used = world
        .dispatch_interaction(Some(index))
        .ok_or("missing interaction")?;
    assert!(
        !world.condition_holds(&enabled),
        "one use consumes interaction capability"
    );
    let _changed = world.dispatch_on(
        "use",
        &[
            ActionDef::SetPrompt {
                target: None,
                prompt: "Used".into(),
            },
            ActionDef::ToggleLabel { target: None },
            ActionDef::Enable {
                target: Some("closed".into()),
            },
            ActionDef::Disable {
                target: Some("authored-state".into()),
            },
        ],
    );
    let handle = world.handle_of("use").ok_or("missing use")?;
    let label = world
        .components()
        .interactables
        .get(handle)
        .ok_or("missing capability")?
        .label_visible;
    let _commands = world.take_commands();
    settle(&mut world, &level);
    let snapshot = world.checkpoint()?;
    let bytes = serde_json::to_vec(&snapshot).map_err(|problem| problem.to_string())?;
    let decoded = serde_json::from_slice(&bytes).map_err(|problem| problem.to_string())?;
    let mut resumed = EntityWorld::from_level(&level);
    resumed.restore_checkpoint(&decoded)?;
    assert!(
        !resumed.condition_holds(&enabled),
        "resume preserves consumed capability until reset"
    );
    assert!(
        !resumed.condition_holds(&closed),
        "resume preserves explicit enable override"
    );
    for candidate in [&mut world, &mut resumed] {
        assert_rearmed(candidate, index, label)?;
    }
    assert!(
        EntityWorld::from_level(&level).condition_holds(&enabled),
        "fresh load remains enabled"
    );
    Ok(())
}

// Two opposite defaults cover leaked repeat modes, rearm without stop, effective
// checkpoint countdowns and reset. Pure simulation ticks cost no wall-clock wait.
#[test]
fn timer_rearm_uses_authored_defaults_and_checkpoint_keeps_only_the_active_override()
-> Result<(), String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"timer_rearm","name":"Timer rearm","spawn":{"x":0,"z":0},"timers":[{"id":"once","seconds":0.6},{"id":"repeat","seconds":0.6,"repeat":true,"autostart":true}]}"#).map_err(|problem| problem.to_string())?;
    for (id, authored) in [("once", false), ("repeat", true)] {
        let mut timers = Timers::from_level(&level);
        assert!(timers.start(id, Some(0.2), Some(!authored)));
        let _partial_tick = timers.tick(0.1, |_| {});
        let encoded =
            serde_json::to_vec(&timers.checkpoint()).map_err(|problem| problem.to_string())?;
        let snapshot: Vec<crate::engine::entities::timers::Checkpoint> =
            serde_json::from_slice(&encoded).map_err(|problem| problem.to_string())?;
        let mut resumed = Timers::from_level(&level);
        resumed.restore_checkpoint(&snapshot)?;
        assert_eq!(
            resumed, timers,
            "checkpoint preserves active mode and remaining time"
        );
        let _checkpoint_tick = resumed.tick(0.11, |_| {});
        assert_eq!(
            resumed.get(id).ok_or("missing resumed timer")?.running,
            !authored,
            "active override survives reconstruction"
        );
        for stop_first in [true, false] {
            assert!(resumed.start(id, Some(0.2), Some(!authored)));
            if stop_first {
                assert!(resumed.stop(id));
            }
            assert!(resumed.start(id, None, None));
            let timer = resumed.get(id).ok_or("missing timer")?;
            assert_eq!(
                timer.repeat(),
                authored,
                "omission restores authored repeat on every start"
            );
            assert!((timer.period() - 0.6).abs() < f32::EPSILON);
            let _default_tick = resumed.tick(0.61, |_| {});
            assert_eq!(
                resumed.get(id).ok_or("missing fired timer")?.running,
                authored
            );
            assert!(
                resumed.start(id, None, Some(false)),
                "explicit false remains meaningful"
            );
            let _explicit_tick = resumed.tick(0.61, |_| {});
            assert!(!resumed.get(id).ok_or("missing stopped timer")?.running);
        }
        let before = resumed.clone();
        assert!(!resumed.start(id, Some(f32::NAN), Some(!authored)));
        assert_eq!(resumed, before, "invalid period cannot mutate the mode");
        resumed.reset();
        assert_eq!(
            resumed,
            Timers::from_level(&level),
            "reset restores authored mode, period, autostart and fires"
        );
    }
    Ok(())
}

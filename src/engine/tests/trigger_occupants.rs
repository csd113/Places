//! Essential metadata-only regressions: physical sweeps must not miss a complete
//! crossing, aggregate handoffs must not stop occupied ambience, and existing
//! binding rearm rules must remain shared with sound actions. These tiny
//! fixtures load no assets, bake no map and need no audio device or GPU.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::events::EventKind;
use crate::engine::entities::{EntityWorld, TriggerItem, WorldCommand, WorldContext};
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};

fn fixture(filter: &str, occupancy: &str, bindings: &str) -> Result<EntityWorld, String> {
    let json = format!(
        r#"{{"format_version":3,"id":"trigger_fixture","name":"Trigger fixture",
        "spawn":{{"x":-5.0,"z":-5.0}},
        "props":[{{"id":"duck_1","model":"core:rubber_duck","components":[
            {{"component":"audio","sound":"duck.test"}}]}}],
        "volumes":[{{"id":"zone","x":0.0,"z":0.0,"width":1.0,"depth":1.0,
            "bottom_y":0.0,"top_y":1.0,"filter":{filter},"occupancy":"{occupancy}",
            "bindings":{bindings}}}]}}"#
    );
    let level = LevelDef::from_json(&json).map_err(|error| error.to_string())?;
    Ok(EntityWorld::from_level(&level))
}

fn item<'a>(key: u64, from: f32, to: f32) -> TriggerItem<'a> {
    TriggerItem {
        key,
        instance: "duck_1",
        asset: "core:rubber_duck",
        from: Vec3::new(from, 0.5, 0.5),
        to: Vec3::new(to, 0.5, 0.5),
        half_extents: Vec3::splat(0.05),
        held: true,
        path: None,
    }
}

fn point() -> Vec3 {
    Vec3::splat(-5.0)
}

const SOUND_BINDINGS: &str = r#"[
    {"on":"enter_volume","actions":[{"action":"play_sound","target":"duck_1"}]},
    {"on":"exit_volume","actions":[{"action":"stop_sound","target":"duck_1"}]}
]"#;

#[test]
fn held_exact_item_sweep_emits_ordered_enter_and_exit() -> Result<(), String> {
    let mut world = fixture(
        r#"{"kind":"item","asset":"core:rubber_duck","instance":"duck_1","held":true}"#,
        "per_actor",
        SOUND_BINDINGS,
    )?;
    let mut wrong = item(1, -2.0, 2.0);
    wrong.held = false;
    world.update_volume_occupants(point(), point(), &[wrong]);
    assert!(
        world.events().is_empty(),
        "unheld items must not match the held filter"
    );
    wrong.held = true;
    wrong.asset = "core:other";
    world.update_volume_occupants(point(), point(), &[wrong]);
    assert!(
        world.events().is_empty(),
        "all supplied exact selectors must match"
    );
    world.update_volume_occupants(point(), point(), &[item(1, -2.0, 2.0)]);
    assert_eq!(
        world.events().len(),
        2,
        "complete fast crossing needs both edges"
    );
    let event = world.events().peek().ok_or("missing swept entry")?;
    assert_eq!(event.kind, EventKind::EnterVolume, "entry precedes exit");
    assert_eq!(
        event.key, "duck_1",
        "event retains the eligible instance identity"
    );
    assert_eq!(
        event.actor,
        world.handle_of("duck_1"),
        "event retains actor handle"
    );
    let _report = world.dispatch_queued_events();
    let commands = world.take_commands();
    assert!(
        matches!(
            commands.as_slice(),
            [
                WorldCommand::PlaySound {
                    entity: _,
                    sound: _,
                    gain: _,
                    looped: _,
                    time_domain: _
                },
                WorldCommand::StopSound { entity: _ }
            ]
        ),
        "a swept entry must not leave looping playback without its exit"
    );
    Ok(())
}

// A two-substep bounce returns to its original endpoint; a chord-only sweep
// cannot detect its real entry. Three points protect that bug at negligible cost.
#[test]
fn substep_bounce_crossing_survives_equal_outside_endpoints() -> Result<(), String> {
    let mut world = fixture(r#"{"kind":"item"}"#, "aggregate", SOUND_BINDINGS)?;
    let path = [
        Vec3::new(-1.0, 0.5, 0.5),
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(-1.0, 0.5, 0.5),
    ];
    let mut bounced = item(1, -1.0, -1.0);
    bounced.path = Some(&path);
    world.update_volume_occupants(point(), point(), &[bounced]);
    assert_eq!(
        world.events().len(),
        2,
        "the intermediate bounce must enter then exit exactly once"
    );
    let _report = world.dispatch_queued_events();
    assert!(
        matches!(
            world.take_commands().as_slice(),
            [
                WorldCommand::PlaySound {
                    entity: _,
                    sound: _,
                    gain: _,
                    looped: _,
                    time_domain: _
                },
                WorldCommand::StopSound { entity: _ }
            ]
        ),
        "touching substep intervals must merge into one crossing"
    );
    Ok(())
}

#[test]
fn aggregate_handoff_and_per_actor_departures_keep_distinct_edges() -> Result<(), String> {
    for (mode, expected_entries, expected_first_exit) in [("aggregate", 1, 0), ("per_actor", 2, 1)]
    {
        let mut world = fixture(r#"{"kind":"item"}"#, mode, SOUND_BINDINGS)?;
        world.update_volume_occupants(point(), point(), &[item(1, -1.0, 0.3), item(2, 2.0, 0.7)]);
        assert_eq!(world.events().len(), expected_entries, "{mode} entry count");
        let _initial_report = world.dispatch_queued_events();
        let _commands = world.take_commands();
        world.update_volume_occupants(point(), point(), &[item(1, 0.3, -1.0), item(2, 0.7, 0.7)]);
        assert_eq!(
            world.events().len(),
            expected_first_exit,
            "{mode} must retain the second occupant"
        );
        let _departure_report = world.dispatch_queued_events();
        world.update_volume_occupants(point(), point(), &[item(2, 0.7, 2.0)]);
        assert_eq!(world.events().len(), 1, "last departure always exits");
    }
    let mut world = fixture(r#"{"kind":"item"}"#, "aggregate", SOUND_BINDINGS)?;
    world.seed_volume_occupants(point(), &[item(1, 0.5, 0.5)]);
    world.update_volume_occupants(point(), point(), &[item(1, 0.5, 2.0), item(2, -0.2, 0.5)]);
    assert!(
        world.events().is_empty(),
        "overlapping swept intervals must not fabricate an aggregate gap"
    );
    Ok(())
}

fn tick(world: &mut EntityWorld, delta: f32) {
    let walls = [];
    let floor = WalkableFloor::default();
    let index = CollisionIndex::build(&walls);
    let _tick = world.tick(&WorldContext {
        delta_seconds: delta,
        feet_from: point(),
        feet: point(),
        eye: point(),
        body_height: 1.8,
        walls: &walls,
        index: &index,
        floor: &floor,
        nav: None,
    });
}

fn crossing(world: &mut EntityWorld) -> usize {
    world.update_volumes(Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5));
    let _report = world.dispatch_queued_events();
    world
        .take_commands()
        .iter()
        .filter(|command| {
            matches!(
                command,
                WorldCommand::PlaySound {
                    entity: _,
                    sound: _,
                    gain: _,
                    looped: _,
                    time_domain: _
                }
            )
        })
        .count()
}

#[test]
fn shared_once_and_cooldown_rearm_without_consuming_exit_bindings() -> Result<(), String> {
    let mut world = fixture(
        r#"{"kind":"player"}"#,
        "per_actor",
        r#"[
        {"on":"enter_volume","once":true,"actions":[{"action":"play_sound","target":"duck_1"}]},
        {"on":"enter_volume","cooldown_seconds":0.5,"actions":[{"action":"play_sound","target":"duck_1"}]},
        {"on":"exit_volume","actions":[{"action":"stop_sound","target":"duck_1"}]}
    ]"#,
    )?;
    assert_eq!(crossing(&mut world), 2, "both initial entries run");
    assert_eq!(
        crossing(&mut world),
        0,
        "once and active cooldown suppress repeated entries"
    );
    tick(&mut world, 0.5);
    assert_eq!(
        crossing(&mut world),
        1,
        "cooldown expires independently of once"
    );
    world.reset_runtime();
    let _commands = world.take_commands();
    world.seed_volumes(point());
    assert_eq!(
        crossing(&mut world),
        2,
        "reset rearms shared binding policy"
    );
    Ok(())
}

#[test]
fn authored_playing_emitter_starts_mutes_resumes_and_restores_on_reset() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"format_version":3,"id":"emitter_fixture","name":"Emitter fixture",
        "spawn":{"x":-5.0,"z":-5.0},
        "props":[{"id":"radio","model":"core:plant","components":[
            {"component":"audio","sound":"ambient.test","playing":true,"looped":true,"time_domain":"real"}]}]}"#)
        .map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    assert!(
        matches!(
            world.take_commands().as_slice(),
            [WorldCommand::PlaySound {
                time_domain: crate::engine::audio::TimeDomain::Real,
                entity: _,
                sound: _,
                gain: _,
                looped: _
            }]
        ),
        "authored playing must issue its initial playback command"
    );
    for (action, expected_play) in [("disable", false), ("enable", true)] {
        let actions: Vec<ActionDef> =
            serde_json::from_str(&format!(r#"[{{"action":"{action}","target":"radio"}}]"#))
                .map_err(|error| error.to_string())?;
        let _report = world.dispatch_actions(&actions, None);
        let commands = world.take_commands();
        assert_eq!(
            commands.iter().any(|command| matches!(
                command,
                WorldCommand::PlaySound {
                    entity: _,
                    sound: _,
                    gain: _,
                    looped: _,
                    time_domain: _
                }
            )),
            expected_play,
            "{action} must apply emitter playback state"
        );
        if !expected_play {
            assert!(
                commands
                    .iter()
                    .any(|command| matches!(command, WorldCommand::StopSound { entity: _ })),
                "disable must stop live audio"
            );
        }
    }
    world.reset_runtime();
    assert!(
        world.take_commands().iter().any(|command| matches!(
            command,
            WorldCommand::PlaySound {
                entity: _,
                sound: _,
                gain: _,
                looped: _,
                time_domain: _
            }
        )),
        "reset restores authored playing state"
    );
    Ok(())
}

// These two bounded queue fixtures protect persistent-loop cleanup under overload.
// They use only 257/4096 tiny in-memory records, never a capacity map or device.
#[test]
fn event_budget_defers_accepted_exit_instead_of_erasing_cleanup() -> Result<(), String> {
    let mut world = fixture(r#"{"kind":"player"}"#, "per_actor", SOUND_BINDINGS)?;
    let zone = world.handle_of("zone").ok_or("missing fixture zone")?;
    for _ in 0..crate::engine::entities::MAX_EVENTS_PER_TICK.saturating_sub(1) {
        assert!(
            world.emit(EventKind::Timer, zone, "", None),
            "fixture prefill fits"
        );
    }
    world.update_volumes(Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5));
    let _first = world.dispatch_queued_events();
    assert_eq!(
        world.events().len(),
        1,
        "budget tail must retain the accepted exit"
    );
    assert!(
        world.take_commands().iter().any(is_play),
        "entry ran at the budget boundary"
    );
    let _second = world.dispatch_queued_events();
    assert!(
        matches!(
            world.take_commands().as_slice(),
            [WorldCommand::StopSound { entity: _ }]
        ),
        "next pump must perform deferred cleanup"
    );
    Ok(())
}

#[test]
fn full_queue_restores_membership_until_final_exit_can_retry() -> Result<(), String> {
    let mut world = fixture(r#"{"kind":"player"}"#, "per_actor", SOUND_BINDINGS)?;
    let zone = world.handle_of("zone").ok_or("missing fixture zone")?;
    world.seed_volumes(Vec3::splat(0.5));
    for _ in 0..crate::engine::entities::events::EventQueue::MAX_QUEUED_EVENTS {
        assert!(
            world.emit(EventKind::Timer, zone, "", None),
            "fixture prefill fits"
        );
    }
    world.update_volumes(Vec3::splat(0.5), point());
    assert!(
        world
            .components()
            .volumes
            .get(zone)
            .is_some_and(|volume| volume.inside),
        "refused exit must preserve membership for retry"
    );
    let _first = world.dispatch_queued_events();
    world.update_volumes(point(), point());
    assert!(
        !world
            .components()
            .volumes
            .get(zone)
            .is_some_and(|volume| volume.inside),
        "available queue space accepts the retried exit"
    );
    while !world.events().is_empty() {
        let _report = world.dispatch_queued_events();
    }
    assert!(
        matches!(
            world.take_commands().as_slice(),
            [WorldCommand::StopSound { entity: _ }]
        ),
        "retried last exit must eventually clean up playback"
    );
    Ok(())
}

const fn is_play(command: &WorldCommand) -> bool {
    matches!(
        command,
        WorldCommand::PlaySound {
            entity: _,
            sound: _,
            gain: _,
            looped: _,
            time_domain: _
        }
    )
}

// Runtime-only emitter tests bypass the source admission gate. This tiny metadata
// regression uniquely protects audio on movable props while retaining the ban
// on competing gameplay components and item-owned bindings; no catalog is read.
#[test]
fn movable_audio_source_admission_preserves_other_component_and_binding_restrictions()
-> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"format_version":3,"id":"movable_audio_fixture","name":"Movable audio fixture",
        "spawn":{"x":-5.0,"z":-5.0},
        "props":[{"id":"duck","model":"core:rubber_duck","solid":true,
            "physics":{"motion":"dynamic","pickup":true},
            "components":[{"component":"audio","sound":"duck.test"}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    crate::engine::loader::validate_prop_physics(&level)?;
    let unsupported: crate::engine::level::ComponentDef =
        serde_json::from_str(r#"{"component":"state","name":"competing","value":true}"#)
            .map_err(|error| error.to_string())?;
    level
        .props
        .first_mut()
        .ok_or("missing fixture prop")?
        .components
        .push(unsupported);
    assert!(
        crate::engine::loader::validate_prop_physics(&level).is_err(),
        "non-audio movable components remain unsupported"
    );
    let prop = level.props.first_mut().ok_or("missing fixture prop")?;
    let _removed = prop.components.pop();
    prop.bindings = serde_json::from_str(SOUND_BINDINGS).map_err(|error| error.to_string())?;
    assert!(
        crate::engine::loader::validate_prop_physics(&level).is_err(),
        "item-owned bindings retain their lifecycle restriction"
    );
    Ok(())
}

// Native feedback consumes the event dispatch report, not only final light state.
// Two presses on two metadata-only entities uniquely protect lost successful
// feedback and false cooldown feedback, without a device, assets or map bake.
#[test]
fn event_light_toggle_reports_feedback_only_when_binding_runs() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"format_version":3,"id":"light_feedback_fixture","name":"Light feedback fixture",
        "spawn":{"x":-5.0,"z":-5.0},
        "props":[{"id":"switch","model":"core:plant","components":[{"component":"interactable"}],
            "bindings":[{"on":"interact","cooldown_seconds":1.0,"actions":[{"action":"toggle","target":"lamp"}]}]}],
        "ceiling_lights":[{"id":"lamp","fixture":"core:fluorescent_panel_01","x":1.0,"z":1.0,"switchable":true}] }"#)
        .map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let target = world
        .interactables()
        .index_of("switch")
        .ok_or("missing fixture switch")?;
    let first = world
        .dispatch_interaction(Some(target))
        .ok_or("missing first dispatch report")?;
    assert_eq!(
        world.light_states(),
        [(0, false)],
        "real event binding must change the fixture"
    );
    assert_eq!(
        first.lights_toggled, 1,
        "event report must retain successful light action for click feedback"
    );
    let second = world
        .dispatch_interaction(Some(target))
        .ok_or("missing second dispatch report")?;
    assert_eq!(
        world.light_states(),
        [(0, false)],
        "cooldown must suppress the second toggle"
    );
    assert_eq!(
        second.lights_toggled, 0,
        "suppressed press must not request feedback"
    );
    Ok(())
}

// One descendant per state transition isolates synchronous chain depth. A small
// bounded FIFO prefix forces a pump deferral; all three action routes must then
// terminate, rather than restarting a runaway chain at depth zero/one forever.
#[test]
fn synchronous_state_children_keep_depth_across_deferred_event_pumps() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{"format_version":3,"id":"depth_fixture","name":"Depth fixture",
        "spawn":{"x":-5.0,"z":-5.0},
        "doors":[{"id":"door","x":0.0,"z":0.0,"width":1.0,"height":2.0,"bindings":[
            {"on":"object_state","key":"locked","when":[{"check":"locked","target":"door"}],"actions":[{"action":"unlock"}]},
            {"on":"object_state","key":"locked","when":[{"check":"unlocked","target":"door"}],"actions":[{"action":"lock"}]}]}],
        "props":[{"id":"control","model":"core:plant","components":[{"component":"interactable"}],"bindings":[
            {"on":"object_state","key":"enabled","when":[{"check":"enabled","target":"control"}],"actions":[{"action":"disable"}]},
            {"on":"object_state","key":"enabled","when":[{"check":"disabled","target":"control"}],"actions":[{"action":"enable"}]}]}],
        "ceiling_lights":[{"id":"lamp","fixture":"core:fluorescent_panel_01","x":2.0,"z":2.0,"switchable":true,"bindings":[
            {"on":"object_state","key":"light","actions":[{"action":"toggle"}]}]}]}"#)
        .map_err(|error| error.to_string())?;
    for (instance, key) in [
        ("door", "locked"),
        ("control", "enabled"),
        ("lamp", "light"),
    ] {
        let mut world = EntityWorld::from_level(&level);
        let subject = world
            .handle_of(instance)
            .ok_or("missing depth fixture entity")?;
        for _ in 0..crate::engine::entities::MAX_EVENTS_PER_TICK.saturating_sub(1) {
            assert!(
                world.emit(EventKind::Timer, subject, "", None),
                "prefix fits bounded queue"
            );
        }
        assert!(
            world.emit(EventKind::ObjectState, subject, key, None),
            "initial state event fits"
        );
        let first = world.dispatch_queued_events();
        assert_eq!(
            first.actions_run, 1,
            "{instance} starts exactly one state descendant"
        );
        assert!(
            world.events().peek().is_some_and(|event| event.depth == 1),
            "{instance} child must retain incremented depth in deferred tail"
        );
        let second = world.dispatch_queued_events();
        assert!(
            second.actions_run > 0,
            "{instance} cycle really executes before the chain guard"
        );
        assert!(
            world.events().is_empty(),
            "{instance} state cycle must terminate at the chain-depth limit"
        );
    }
    Ok(())
}

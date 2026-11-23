//! Essential metadata-only regressions: physical sweeps must not miss a complete
//! crossing, aggregate handoffs must not stop occupied ambience, and existing
//! binding rearm rules must remain shared with sound actions. These tiny
//! fixtures load no assets, bake no map and need no audio device or GPU.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::events::{EventKind, EventQueue};
use crate::engine::entities::{EntityWorld, TriggerItem, WorldCommand, WorldContext};
use crate::engine::level::{ActionDef, LevelDef, WalkableFloor};

fn fixture(filter: &str, occupancy: &str, bindings: &str) -> Result<EntityWorld, String> {
    let json = format!(
        r#"{{"schema": "places-level", "format_version":1,"id":"trigger_fixture","name":"Trigger fixture",
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
        sleeping: false,
        up_y: 1.0,
        centre_path: None,
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

// A four-point solver path crosses an earlier-authored far volume after leaving,
// reentering and leaving the near volume. One bounded FIFO prefix forces the
// records to survive two dispatch frames without changing their exit positions.
#[test]
fn solved_crossings_keep_exact_exit_positions_and_global_fifo() -> Result<(), String> {
    use crate::engine::entities::MAX_EVENTS_PER_TICK;

    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"crossings","name":"Crossings",
        "spawn":{"x":-5,"z":-5},
        "volumes":[
            {"id":"far","x":-2,"z":0,"width":0.5,"depth":1,"bottom_y":0,"top_y":1,"filter":{"kind":"item"}},
            {"id":"near","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1,"filter":{"kind":"item"}}
        ]
    }"#).map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let near = world.handle_of("near").ok_or("missing near volume")?;
    let far = world.handle_of("far").ok_or("missing far volume")?;
    let path = [
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(2.0, 0.5, 0.5),
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(-3.0, 0.5, 0.5),
    ];
    let mut crossed = item(1, 0.5, -3.0);
    crossed.path = Some(&path);
    for prefix in [
        MAX_EVENTS_PER_TICK.saturating_add(1),
        EventQueue::MAX_QUEUED_EVENTS.saturating_sub(3),
    ] {
        world.seed_volume_occupants(point(), &[item(1, 0.5, 0.5)]);
        let drops_before = world.events().dropped();
        for _ in 0..prefix {
            assert!(world.emit(EventKind::Timer, far, "", None), "backlog fits");
        }
        world.update_volume_occupants(point(), point(), &[crossed]);
        let _first = world.dispatch_queued_events();
        assert!(
            world
                .events()
                .peek()
                .is_some_and(|event| event.kind == EventKind::Timer),
            "the earliest prior-frame event still heads the queue"
        );
        let mut queued = world.events().clone();
        let mut crossings = Vec::new();
        while let Some(event) = queued.pop() {
            if event.kind != EventKind::Timer {
                crossings.push((event.subject, event.kind, event.origin));
            }
        }
        let expected = [
            (near, EventKind::ExitVolume, Some(1.05)),
            (near, EventKind::EnterVolume, None),
            (near, EventKind::ExitVolume, Some(-0.05)),
            (far, EventKind::EnterVolume, None),
            (far, EventKind::ExitVolume, Some(-2.05)),
        ];
        let admitted = EventQueue::MAX_QUEUED_EVENTS
            .saturating_sub(prefix)
            .min(expected.len());
        assert_eq!(crossings.len(), admitted, "only the earliest crossings fit");
        assert_eq!(
            world.events().dropped().saturating_sub(drops_before),
            expected.len().saturating_sub(admitted),
            "the later crossings are the only drops"
        );
        for ((subject, kind, origin), (expected_subject, expected_kind, expected_x)) in
            crossings.into_iter().zip(expected)
        {
            assert_eq!(
                (subject, kind),
                (expected_subject, expected_kind),
                "crossing order and identity"
            );
            if let Some(x) = expected_x {
                assert!(
                    origin
                        .is_some_and(|position| position.distance(Vec3::new(x, 0.5, 0.5)) < 1.0e-5),
                    "exit must come from its own solved segment: {origin:?}"
                );
            }
        }
        drain_trigger_events(&mut world);
    }
    Ok(())
}

// Two route legs cross a small volume that the frame chord entirely misses.
// A single route update proves watched non-physical entities use accepted legs.
#[test]
fn watched_route_crosses_before_turning_away() -> Result<(), String> {
    use crate::engine::entity::RouteWorld;

    let level = LevelDef::from_json(
        r#"{
        "schema":"places-level","format_version":1,"id":"route_trigger","name":"Route trigger",
        "spawn":{"x":3,"z":3},"rooms":[{"width":4,"depth":4}],
        "props":[{"id":"actor","model":"mannequin","x":1,"z":1,"solid":false}],
        "routes":[{"id":"actor","steps":[
            {"step":"move_to","x":1.25,"z":1,"speed":12},
            {"step":"move_to","x":1.25,"z":1.25,"speed":12}
        ]}],
        "volumes":[{"id":"zone","x":1.15,"z":0.98,"width":0.05,"depth":0.04,
            "bottom_y":-0.1,"top_y":0.1,"filter":{"kind":"entity","instance":"actor"}}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    world.seed_volumes(point());
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::build(&[]);
    world.update_entities(
        0.1,
        &RouteWorld {
            walls: &[],
            floor: &floor,
            index: &index,
        },
        Some(point()),
    );
    world.update_volumes(point(), point());

    let mut events = world.events().clone();
    assert_eq!(events.len(), 2, "the accepted first leg enters and exits");
    assert!(
        events
            .pop()
            .is_some_and(|event| event.kind == EventKind::EnterVolume),
        "entry comes first"
    );
    let exit = events.pop().ok_or("missing routed exit")?;
    assert_eq!(exit.kind, EventKind::ExitVolume, "the next edge is an exit");
    assert!(
        exit.origin
            .is_some_and(|origin| origin.distance(Vec3::new(1.2, 0.0, 1.0)) < 1.0e-5),
        "the exit is on the accepted first leg: {:?}",
        exit.origin
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
        player_walk_speed: 3.75,
        delta_seconds: delta,
        feet_from: point(),
        feet: point(),
        eye: point(),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
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
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"emitter_fixture","name":"Emitter fixture",
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

// This 256-record prefix protects admitted cleanup at the dispatch budget
// boundary without building a capacity-size map or opening an audio device.
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

fn overflow_loop_fixture() -> Result<EntityWorld, String> {
    use crate::engine::project::ProjectPath;
    use crate::engine::scripting::ScriptSource;

    // One volume, two emitters and one callback isolate real playback ownership
    // from exit effects and use only the bounded FIFO to force overflow.
    let mut level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"exit_fixture","name":"Exit fixture",
        "spawn":{"x":-5,"z":-5},
        "props":[
            {"id":"loop","model":"core:rubber_duck","components":[{"component":"audio","sound":"loop.test"}]},
            {"id":"other","model":"core:rubber_duck","components":[{"component":"audio","sound":"other.test","playing":true}]}
        ],
        "volumes":[{"id":"zone","x":0,"z":0,"width":1,"depth":1,"bottom_y":0,"top_y":1,
            "bindings":[
                {"on":"enter_volume","actions":[{"action":"play_sound","target":"loop","loop":true}]},
                {"on":"exit_volume","once":true,"actions":[
                    {"action":"stop_sound","target":"loop"},
                    {"action":"stop_sound","target":"other"},
                    {"action":"run_script","script":"scripts/exit.luau","handler":"leave"}
                ]}
            ]}]
    }"#).map_err(|error| error.to_string())?;
    level.script_sources.push(ScriptSource {
        path: ProjectPath::new("scripts/exit.luau".into())?,
        source: r#"return {leave=function(api) api.message("real exit", {"marker"}) end}"#.into(),
    });
    let mut world = EntityWorld::from_level(&level);
    let _autoplay = world.take_commands();
    Ok(world)
}

fn drain_trigger_events(world: &mut EntityWorld) {
    while !world.events().is_empty() {
        let _report = world.dispatch_queued_events();
    }
}

fn fill_trigger_backlog(
    world: &mut EntityWorld,
    zone: crate::engine::entities::id::EntityHandle,
    slots_left: usize,
) {
    for _ in 0..EventQueue::MAX_QUEUED_EVENTS.saturating_sub(slots_left) {
        assert!(
            world.emit(EventKind::Timer, zone, "", None),
            "fixture prefill fits exactly"
        );
    }
}

#[test]
fn overflow_stops_only_the_departed_trigger_loop_without_replaying_exit() -> Result<(), String> {
    let mut world = overflow_loop_fixture()?;
    let zone = world.handle_of("zone").ok_or("missing fixture zone")?;
    let active = world.handle_of("loop").ok_or("missing fixture loop")?;
    let other = world
        .handle_of("other")
        .ok_or("missing unrelated emitter")?;
    let inside = Vec3::splat(0.5);
    let outside = Vec3::new(-1.0, 0.5, 0.5);
    world.update_volumes(outside, inside);
    let _entry = world.dispatch_queued_events();
    let entry_commands = world.take_commands();
    let [
        WorldCommand::PlaySound {
            entity,
            looped: true,
            sound: _,
            gain: _,
            time_domain: _,
        },
    ] = entry_commands.as_slice()
    else {
        return Err("entry did not start the fixture loop".into());
    };

    fill_trigger_backlog(&mut world, zone, 0);
    world.update_volumes(inside, outside);
    assert_eq!(
        world.events().len(),
        EventQueue::MAX_QUEUED_EVENTS,
        "capacity is shared"
    );
    assert_eq!(
        world.events().dropped(),
        1,
        "only the newest exit is refused"
    );
    assert!(
        world
            .components()
            .volumes
            .get(zone)
            .is_some_and(|volume| !volume.inside),
        "actual departure updates occupancy despite overflow"
    );
    assert!(
        world
            .components()
            .audio
            .get(active)
            .is_some_and(|emitter| !emitter.playing),
        "the departed visit clears its emitter state immediately"
    );
    assert!(
        world
            .components()
            .audio
            .get(other)
            .is_some_and(|emitter| emitter.playing),
        "an unrelated emitter stays active despite its authored exit action"
    );
    assert!(
        matches!(world.take_commands().as_slice(), [WorldCommand::StopSound { entity: stopped }] if stopped == entity),
        "only the owned loop stops before a dispatch pump"
    );

    let _first = world.dispatch_queued_events();
    let remaining = world.events().len();
    world.update_volumes(outside, outside);
    assert_eq!(
        world.events().len(),
        remaining,
        "newly available space cannot replay an exit"
    );
    drain_trigger_events(&mut world);
    assert!(
        world.take_commands().is_empty(),
        "no exit action or script ran later"
    );
    world.update_volumes(outside, inside);
    let _next_entry = world.dispatch_queued_events();
    assert!(
        world.take_commands().iter().any(is_play),
        "a later entry starts a fresh loop"
    );
    world.update_volumes(inside, outside);
    let _real_exit = world.dispatch_queued_events();
    let commands = world.take_commands();
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, WorldCommand::StopSound { entity: _ }))
            .count(),
        2,
        "a real admitted exit still runs both authored audio actions"
    );
    assert!(commands.iter().any(|command| matches!(command, WorldCommand::ShowMessage { title, lines: _ } if title == "real exit")),
        "the dropped exit did not consume the once-only script binding");

    Ok(())
}

// A single spared queue slot forces the admitted entry to outlive its dropped
// exit. The delayed entry must not start a loop after the real visit ended.
#[test]
fn dropped_exit_cancels_loop_start_of_an_admitted_entry() -> Result<(), String> {
    let mut world = overflow_loop_fixture()?;
    let zone = world.handle_of("zone").ok_or("missing fixture zone")?;
    let outside = Vec3::new(-1.0, 0.5, 0.5);
    fill_trigger_backlog(&mut world, zone, 1);
    world.update_volumes(outside, Vec3::new(2.0, 0.5, 0.5));
    assert_eq!(
        world.events().dropped(),
        1,
        "the paired exit is dropped behind its admitted entry"
    );
    drain_trigger_events(&mut world);
    assert!(
        world.take_commands().is_empty(),
        "the delayed entry cannot restart a departed visit's loop"
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
        r#"{"schema": "places-level", "format_version":1,"id":"movable_audio_fixture","name":"Movable audio fixture",
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
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"light_feedback_fixture","name":"Light feedback fixture",
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
    let level = LevelDef::from_json(r#"{"schema": "places-level", "format_version":1,"id":"depth_fixture","name":"Depth fixture",
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

// This adds one real, rotating 0.6 m box and a metadata receiver. It protects
// against conservative sphere false receipts plus held/sleep/reset transitions;
// four solver steps and a handful of edge updates require no bake or device.
#[test]
fn centre_and_settled_receipts_reject_rotated_envelopes_and_rearm_cleanly() -> Result<(), String> {
    use crate::engine::level::ItemReceiptDef;
    let body = rotating_receipt_body()?;
    assert!(
        body.half_extents.y > 0.3,
        "rotating box retains its safe overlap envelope"
    );
    assert!(
        body.centre_path().iter().all(|point| point.y > 1.0),
        "actual box centre remains above receiver"
    );
    let rotated = TriggerItem {
        from: body.previous,
        to: body.current,
        half_extents: body.half_extents,
        path: Some(body.path()),
        centre_path: Some(body.centre_path()),
        held: false,
        sleeping: body.sleeping,
        ..item(1, 0.5, 0.5)
    };
    for (receipt, expected) in [(ItemReceiptDef::Overlap, 1), (ItemReceiptDef::Centre, 0)] {
        let mut world = fixture(
            r#"{"kind":"item","instance":"duck_1","held":false}"#,
            "per_actor",
            SOUND_BINDINGS,
        )?;
        let zone = world.handle_of("zone").ok_or("missing receiver")?;
        world
            .components_mut()
            .volumes
            .get_mut(zone)
            .ok_or("missing volume")?
            .item_receipt = receipt;
        world.update_volume_occupants(point(), point(), &[rotated]);
        assert_eq!(
            world.events().len(),
            expected,
            "centre receipt rejects the rotation envelope only"
        );
    }
    let mut world = fixture(
        r#"{"kind":"item","instance":"duck_1","held":false}"#,
        "per_actor",
        SOUND_BINDINGS,
    )?;
    let zone = world.handle_of("zone").ok_or("missing receiver")?;
    world
        .components_mut()
        .volumes
        .get_mut(zone)
        .ok_or("missing volume")?
        .item_receipt = ItemReceiptDef::SettledCentre;
    let mut resting = item(1, 0.5, 0.5);
    for (held, sleeping, expected) in [
        (true, false, None),
        (false, false, None),
        (false, true, Some(EventKind::EnterVolume)),
        (true, true, Some(EventKind::ExitVolume)),
        (false, true, Some(EventKind::EnterVolume)),
        (false, false, Some(EventKind::ExitVolume)),
    ] {
        resting.held = held;
        resting.sleeping = sleeping;
        world.update_volume_occupants(point(), point(), &[resting]);
        assert_eq!(
            world.events().peek().map(|event| event.kind),
            expected,
            "held and wake transitions update receipt at rest"
        );
        let _ = world.dispatch_queued_events();
    }
    resting.sleeping = true;
    world.reset_runtime();
    world.seed_volume_occupants(point(), &[resting]);
    world.update_volume_occupants(point(), point(), &[resting]);
    assert!(
        world.events().is_empty(),
        "reset seed does not invent a new receipt"
    );
    world.update_volumes(point(), point());
    assert_eq!(
        world.events().peek().map(|event| event.kind),
        Some(EventKind::ExitVolume),
        "missing settled item releases receipt"
    );
    Ok(())
}

fn rotating_receipt_body() -> Result<crate::engine::physics::ItemTriggerBody, String> {
    use crate::engine::physics::{BodyMotion, FIXED_STEP, ItemWorld, PhysicsDef, PlayerContext};
    let mut physics = ItemWorld::default();
    physics.spawn(
        1,
        &PhysicsDef {
            motion: BodyMotion::Dynamic,
            pickup: true,
            ..PhysicsDef::default()
        },
        (Vec3::new(-0.3, -0.05, -0.05), Vec3::new(0.3, 0.05, 0.05)),
        Vec3::new(0.5, 1.2, 0.5),
        0.0,
        1.0,
    )?;
    assert!(
        physics.set_fixture_velocity(1, Vec3::ZERO, Vec3::new(0.0, 4.0, 0.0)),
        "bounded angular velocity must be accepted"
    );
    physics.step(
        FIXED_STEP * 4.0,
        PlayerContext {
            half: crate::engine::collision::PLAYER_RADIUS,
            eye: point(),
            feet: point(),
            height: 1.8,
            view: Vec3::NEG_Z,
            yaw: 0.0,
            active: false,
        },
        &[],
    );
    let mut snapshot = None;
    physics.for_each_trigger_body(|body| snapshot = Some(body));
    snapshot.ok_or_else(|| "missing rotated box".to_string())
}

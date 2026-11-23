//! Essential lifecycle regressions for lost starts, uncancellable in-flight
//! runs, missing completion keys/reset reports and stale movement controllers.
//! Two metadata-only props and a flat floor exercise the public world API in
//! milliseconds, without loading assets, baking maps or allocating a GPU.

use glam::Vec3;

use crate::engine::collision_index::CollisionIndex;
use crate::engine::entities::{EntityWorld, WorldContext, WorldTick};
use crate::engine::level::{ActionDef, ConditionDef, LevelDef, WalkableFloor};

fn fixture(sequences: &str, bindings: &str) -> Result<(EntityWorld, WalkableFloor), String> {
    let json = format!(
        r#"{{"schema": "places-level", "format_version":1,"id":"sequence_fixture","name":"Sequence fixture",
        "spawn":{{"x":3.0,"z":3.0}},
        "rooms":[{{"x":0.0,"z":0.0,"width":4.0,"depth":4.0}}],
        "props":[
            {{"id":"first","model":"core:rubber_duck","x":0.5,"z":0.5,
                "components":[{{"component":"state","name":"phase","value":"idle"}}],
                "bindings":{bindings}}},
            {{"id":"second","model":"core:rubber_duck","x":1.5,"z":0.5,
                "components":[{{"component":"state","name":"phase","value":"idle"}}]}}
        ],"sequences":{sequences}}}"#
    );
    let level = LevelDef::from_json(&json).map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    Ok((EntityWorld::from_level(&level), floor))
}

fn tick(world: &mut EntityWorld, seconds: f32, floor: &WalkableFloor) -> WorldTick {
    tick_with_nav(world, seconds, floor, None)
}

fn tick_with_nav(
    world: &mut EntityWorld,
    seconds: f32,
    floor: &WalkableFloor,
    nav: Option<&crate::engine::nav::NavMesh>,
) -> WorldTick {
    let index = CollisionIndex::build(&[]);
    world.tick(&WorldContext {
        player_walk_speed: 3.75,
        delta_seconds: seconds,
        feet_from: Vec3::new(3.0, 0.0, 3.0),
        feet: Vec3::new(3.0, 0.0, 3.0),
        eye: Vec3::new(3.0, 1.6, 3.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor,
        nav,
    })
}

fn start(world: &mut EntityWorld, owner: &str, sequence: &str) {
    let report = world.dispatch_on(
        owner,
        &[ActionDef::StartSequence {
            sequence: sequence.to_string(),
            target: None,
        }],
    );
    assert_eq!(report.sequences_started, 1, "fixture sequence must start");
}

fn phase<'a>(world: &'a EntityWorld, owner: &str) -> Result<&'a str, String> {
    let handle = world.handle_of(owner).ok_or("fixture owner missing")?;
    world
        .components()
        .states
        .get(handle)
        .and_then(|state| state.get("phase"))
        .and_then(crate::engine::entities::components::StateValue::as_text)
        .ok_or_else(|| "fixture state missing".to_string())
}

fn idle(world: &EntityWorld, owner: &str) -> bool {
    world.condition_holds(&ConditionDef::SequenceIdle {
        target: owner.to_string(),
    })
}

#[test]
fn sequence_action_start_on_another_owner_survives_the_tick() -> Result<(), String> {
    let (mut world, floor) = fixture(
        r#"[
        {"id":"parent","steps":[
            {"step":"action","action":{"action":"start_sequence","sequence":"child","target":"second"}},
            {"step":"stop"}]},
        {"id":"child","steps":[{"step":"set_state","name":"phase","value":"child"}]}
    ]"#,
        "[]",
    )?;
    start(&mut world, "first", "parent");
    let first = tick(&mut world, 0.1, &floor);
    assert_eq!(
        first.sequences_completed, 1,
        "only the original run completes"
    );
    assert_eq!(
        world.sequence_count(),
        1,
        "the newly started owner must remain scheduled"
    );
    assert_eq!(
        phase(&world, "second")?,
        "idle",
        "new starts run on the following tick"
    );
    let second = tick(&mut world, 0.1, &floor);
    assert_eq!(
        phase(&world, "second")?,
        "child",
        "the new owner must execute its steps"
    );
    assert_eq!(
        second.sequences_completed, 1,
        "the new run completes exactly once"
    );
    assert_eq!(world.sequence_count(), 0, "completed runs must be reaped");
    Ok(())
}

#[test]
fn replacement_and_self_stop_cancel_the_inflight_tail() -> Result<(), String> {
    for (action, expected_runs, expected_phase) in [
        (
            r#"{"action":"start_sequence","sequence":"replacement"}"#,
            1,
            "replacement",
        ),
        (
            r#"{"action":"start_sequence","sequence":"original"}"#,
            1,
            "idle",
        ),
        (r#"{"action":"stop_sequence"}"#, 0, "idle"),
    ] {
        let sequences = format!(
            r#"[
            {{"id":"original","steps":[
                {{"step":"wait","seconds":0.05}},
                {{"step":"action","action":{action}}},
                {{"step":"set_state","name":"phase","value":"stale"}}]}},
            {{"id":"replacement","steps":[
                {{"step":"set_state","name":"phase","value":"replacement"}}]}}
        ]"#
        );
        let (mut world, floor) = fixture(&sequences, "[]")?;
        start(&mut world, "first", "original");
        let first = tick(&mut world, 0.1, &floor);
        assert_eq!(
            first.actions_run, 1,
            "the live run must accept its own control action"
        );
        assert_eq!(
            first.sequences_completed, 0,
            "cancellation must not emit completion"
        );
        assert_eq!(
            phase(&world, "first")?,
            "idle",
            "the old tail must never execute"
        );
        assert_eq!(
            world.sequence_count(),
            expected_runs,
            "the scheduler must retain only the replacement"
        );
        assert_eq!(
            idle(&world, "first"),
            expected_runs == 0,
            "controller state must match the live run"
        );
        let second = tick(&mut world, 0.1, &floor);
        if expected_runs == 1 {
            assert_eq!(
                phase(&world, "first")?,
                expected_phase,
                "the replacement executes without restoring its predecessor"
            );
            assert_eq!(
                second.sequences_completed,
                usize::from(expected_phase == "replacement"),
                "only the replacement that reaches its end completes"
            );
        } else {
            assert_eq!(
                phase(&world, "first")?,
                "idle",
                "a self-stopped run must stay stopped"
            );
            assert_eq!(
                second.sequences_completed, 0,
                "a stopped run cannot complete later"
            );
        }
    }
    Ok(())
}

#[test]
fn completion_key_and_pause_preserve_the_authored_contract() -> Result<(), String> {
    let (mut world, floor) = fixture(
        r#"[{"id":"complete","steps":[{"step":"stop"}]}]"#,
        r#"[{"on":"sequence_complete","key":"complete","actions":[
            {"action":"set_state","name":"phase","value":"complete"}]}]"#,
    )?;
    start(&mut world, "first", "complete");
    let paused = tick(&mut world, 0.0, &floor);
    assert_eq!(
        paused.sequences_completed, 0,
        "zero simulation time must freeze immediate steps too"
    );
    assert_eq!(
        phase(&world, "first")?,
        "idle",
        "a paused sequence must not dispatch completion"
    );
    assert!(!idle(&world, "first"), "paused running state must survive");
    let resumed = tick(&mut world, 0.1, &floor);
    assert_eq!(
        resumed.sequences_completed, 1,
        "resuming completes the run once"
    );
    assert_eq!(
        phase(&world, "first")?,
        "complete",
        "the documented sequence-id key must match"
    );
    assert!(idle(&world, "first"), "completion clears the controller");
    assert_eq!(
        tick(&mut world, 0.1, &floor).sequences_completed,
        0,
        "completion cannot repeat"
    );
    start(&mut world, "first", "complete");
    world.reset_runtime();
    assert_eq!(
        world.sequence_count(),
        0,
        "reset cancels paused and active runs"
    );
    assert_eq!(
        phase(&world, "first")?,
        "idle",
        "reset restores authored state"
    );
    assert!(
        idle(&world, "first"),
        "reset clears the sequence controller"
    );
    assert_eq!(
        tick(&mut world, 0.1, &floor).sequences_completed,
        0,
        "cancelled reset work cannot return"
    );
    Ok(())
}

#[test]
fn sequence_reset_reaches_the_frame_loop_and_aborts_remaining_steps() -> Result<(), String> {
    let (mut world, floor) = fixture(
        r#"[
        {"id":"reset","steps":[
            {"step":"action","action":{"action":"reset_to_start"}},
            {"step":"set_state","name":"phase","value":"stale"}]},
        {"id":"other","steps":[{"step":"set_state","name":"phase","value":"stale"}]}
    ]"#,
        "[]",
    )?;
    start(&mut world, "first", "reset");
    start(&mut world, "second", "other");
    let report = tick(&mut world, 0.1, &floor);
    assert!(
        report.player_reset,
        "sequence reset must reach the native frame loop"
    );
    assert_eq!(report.actions_run, 1, "the reset action must be counted");
    assert_eq!(
        phase(&world, "first")?,
        "idle",
        "the reset run's tail must stop"
    );
    assert_eq!(
        phase(&world, "second")?,
        "idle",
        "later owners must wait for reset handling"
    );
    assert_eq!(
        report.sequences_completed, 0,
        "reset must not spuriously finish pending work"
    );
    Ok(())
}

#[test]
fn movement_cancellation_clears_the_controller_without_completing() -> Result<(), String> {
    let (mut world, _floor) = fixture(
        r#"[
        {"id":"move","steps":[
            {"step":"move","x":1.5,"z":0.5,"speed":1.0},
            {"step":"set_state","name":"phase","value":"unreachable"}]}
    ]"#,
        "[]",
    )?;
    start(&mut world, "first", "move");
    let report = tick(&mut world, 0.1, &WalkableFloor::default());
    assert_eq!(
        world.sequence_count(),
        0,
        "a move into missing floor must cancel"
    );
    assert!(
        idle(&world, "first"),
        "cancelled movement must release its controller"
    );
    assert_eq!(
        report.sequences_completed, 0,
        "blocked cancellation has no completion event"
    );
    assert_eq!(
        phase(&world, "first")?,
        "idle",
        "cancelled movement must not run its tail"
    );
    Ok(())
}

#[test]
fn sequence_movement_and_turns_publish_frame_changes() -> Result<(), String> {
    let (mut world, floor) = fixture(
        r#"[
        {"id":"move","steps":[{"step":"move","x":1.5,"z":0.5,"speed":1.0}]},
        {"id":"face","steps":[{"step":"face","yaw_degrees":90.0}]}
    ]"#,
        "[]",
    )?;
    start(&mut world, "first", "move");
    start(&mut world, "second", "face");
    let first = world.handle_of("first").ok_or("missing mover")?;
    let second = world.handle_of("second").ok_or("missing turner")?;
    let moved = tick(&mut world, 0.1, &floor);
    assert!(
        moved.frames_dirty,
        "sequence transforms must refresh renderer frames"
    );
    let position = world
        .components()
        .transforms
        .get(first)
        .ok_or("missing moved transform")?
        .position;
    let yaw = world
        .components()
        .transforms
        .get(second)
        .ok_or("missing turned transform")?
        .yaw_degrees;
    assert!(
        position.x > 0.5,
        "movement must advance on the walkable floor"
    );
    assert!(yaw > 0.0, "facing must advance toward its authored yaw");
    let paused = tick(&mut world, 0.0, &floor);
    assert!(
        !paused.frames_dirty,
        "paused transforms must need no refresh"
    );
    assert!(
        world
            .components()
            .transforms
            .get(first)
            .is_some_and(|transform| transform.position == position),
        "pause preserves movement position"
    );
    assert!(
        world
            .components()
            .transforms
            .get(second)
            .is_some_and(|transform| (transform.yaw_degrees - yaw).abs() < 1.0e-6),
        "pause preserves turn angle"
    );
    Ok(())
}

// Existing small world fixture plus one runtime spawn and one live solid
// distinguishes cancellation from legacy retry, and checks upper-floor travel.
#[test]
fn blocked_scripted_moves_require_reissue_and_keep_their_floor() -> Result<(), String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"move_upper","name":"Move upper",
        "spawn":{"x":0.5,"z":0.5},"rooms":[{"width":5,"depth":3,"height":4},{"width":3,"depth":3,"height":3,"floor_y":4}],
        "props":[{"id":"first","model":"core:rubber_duck","x":0.5,"y":4,"z":0.5}],
        "spawn_templates":[{"id":"prop","model":"core:rubber_duck"}],
        "spawn_points":[{"id":"point","x":0.5,"y":4,"z":1.5,"template":"prop"}],
        "sequences":[{"id":"move","steps":[{"step":"move","x":2.5,"z":0.5,"speed":2}]}]}"#)
        .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let mut world = EntityWorld::from_level(&level);
    let spawn = world.dispatch_actions(
        &[ActionDef::SpawnEntity {
            point: Some("point".into()),
            name: Some("moving".into()),
            template: None,
            group: None,
        }],
        None,
    );
    assert_eq!(spawn.spawned, 1);
    let scripted = ActionDef::MoveObject {
        target: Some("moving".into()),
        x: 2.5,
        y: None,
        z: 1.5,
        speed: Some(2.0),
    };
    let position = |state: &EntityWorld, name: &str| -> Result<Vec3, String> {
        state
            .handle_of(name)
            .and_then(|handle| state.components().transforms.get(handle))
            .map(|transform| transform.position)
            .ok_or_else(|| format!("missing mover {name}"))
    };
    let blocked = crate::engine::collision::WallAabb::with_y(0.9, 4.0, 0.0, 0.04, 2.5, 2.0);
    world.set_navigation_obstacles(std::iter::once(blocked));
    start(&mut world, "first", "move");
    assert_eq!(
        world
            .dispatch_actions(std::slice::from_ref(&scripted), None)
            .objects_moved,
        1
    );
    for _ in 0..8_u8 {
        let _ = tick(&mut world, 0.10, &floor);
    }
    assert!(idle(&world, "first"));
    let before = [position(&world, "first")?, position(&world, "moving")?];
    world.set_navigation_obstacles(std::iter::empty());
    for _ in 0..4_u8 {
        let _ = tick(&mut world, 0.10, &floor);
    }
    assert_eq!(
        [position(&world, "first")?, position(&world, "moving")?],
        before
    );
    start(&mut world, "first", "move");
    assert_eq!(world.dispatch_actions(&[scripted], None).objects_moved, 1);
    for _ in 0..16_u8 {
        let _ = tick(&mut world, 0.10, &floor);
    }
    for id in ["first", "moving"] {
        let accepted = position(&world, id)?;
        assert!(accepted.x > 2.45 && (accepted.y - 4.0).abs() < 1.0e-5);
    }
    Ok(())
}

// One actual AI owner protects the cross-system bug that metadata transforms
// moved while renderer frames and resumed AI retained the old authored pose.
// This adds no navigation bake or asset load to the tiny CPU-only fixture.
#[test]
fn sequence_pose_reaches_ai_and_render_frames_and_survives_release() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
        "schema": "places-level", "format_version":1,"id":"sequence_ai_fixture","name":"Sequence AI fixture",
        "spawn":{"x":3.0,"z":3.0},
        "rooms":[{"x":0.0,"z":0.0,"width":4.0,"depth":4.0}],
        "props":[{"id":"actor","model":"core:rubber_duck","x":0.5,"z":0.5,
            "components":[
                {"component":"nav_agent","radius":0.2,"speed_mps":1.0},
                {"component":"ai","behavior":"idler","sight_range":0.0,"hearing_range":0.0}]}],
        "sequences":[{"id":"performance","after_move":"stay","steps":[
            {"step":"move","x":0.7,"z":0.5,"speed":1.0},
            {"step":"face","yaw_degrees":90.0},
            {"step":"wait","seconds":0.2},
            {"step":"stop"}]}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let mut world = EntityWorld::from_level(&level);
    let handle = world.handle_of("actor").ok_or("missing AI actor")?;
    start(&mut world, "actor", "performance");
    let mut saw_movement = false;
    let mut saw_turn = false;
    for _ in 0_u8..20 {
        let _report = tick(&mut world, 0.1, &floor);
        let transform = world
            .components()
            .transforms
            .get(handle)
            .ok_or("missing sequence transform")?;
        let agent = world.ai().agent("actor").ok_or("missing sequence AI")?;
        let (frame_position, frame_yaw) = world
            .entity_frames()
            .iter()
            .find(|frame| frame.instance_id == "actor")
            .and_then(|frame| frame.transform)
            .ok_or("missing sequence renderer transform")?;
        assert!(
            agent.position.distance(transform.position) < 1.0e-6,
            "AI perception must use the sequence's live feet position"
        );
        assert!(
            (agent.yaw_degrees - transform.yaw_degrees).abs() < 1.0e-6,
            "AI must retain the sequence's current turn"
        );
        assert!(
            frame_position.distance(transform.position) < 1.0e-6,
            "renderer frames must display the sequence movement"
        );
        assert!(
            (frame_yaw - transform.yaw_degrees.to_radians()).abs() < 1.0e-6,
            "renderer frames must display the sequence turn in radians"
        );
        saw_movement |= agent.position.x > 0.5;
        saw_turn |= agent.yaw_degrees > 0.0;
        if world.sequence_count() == 0 {
            break;
        }
        assert!(
            matches!(agent.state, crate::engine::ai::AiState::Scripted),
            "the running sequence owns AI locomotion"
        );
    }
    assert!(
        saw_movement && saw_turn,
        "the fixture must exercise movement and facing"
    );
    assert_eq!(
        world.sequence_count(),
        0,
        "the performance must finish in its bounded time"
    );
    let _resumed = tick(&mut world, 0.1, &floor);
    let agent = world.ai().agent("actor").ok_or("missing released AI")?;
    assert!(
        matches!(agent.state, crate::engine::ai::AiState::Idle),
        "completion releases the ordinary idler state"
    );
    assert!(
        agent.position.distance(Vec3::new(0.7, 0.0, 0.5)) < 1.0e-6,
        "release must keep the scripted destination instead of the old authored position"
    );
    assert!(
        (agent.yaw_degrees - 90.0).abs() < 1.0e-6,
        "release must keep the completed turn"
    );
    Ok(())
}

// These three cinematic regressions exercise unique ownership boundaries not
// covered by ordinary sequence tests: exclusive view leases, completion queue
// backpressure, and generational live camera references. They reuse two tiny
// metadata entities, no models/GPU/bake; only the queue test fills the existing
// bounded 4096-entry FIFO once.
const CINEMATIC: &str = r#"[{"id":"shot","cinematic":{"start":{"position":[0,2,0],"yaw_degrees":0,"pitch_degrees":0}},"steps":[{"step":"camera","seconds":1,"motion":{"kind":"move","position":[2,2,0],"easing":"linear"}},{"step":"stop"}]}]"#;

#[test]
fn cinematic_lease_rejects_restart_pauses_and_restores_on_cancel() -> Result<(), String> {
    let (mut world, floor) = fixture(CINEMATIC, "[]")?;
    start(&mut world, "first", "shot");
    let token = world.cinematic_token().ok_or("missing start lease")?;
    for owner in ["first", "second"] {
        let report = world.dispatch_on(
            owner,
            &[ActionDef::StartSequence {
                sequence: "shot".into(),
                target: None,
            }],
        );
        assert_eq!(
            report.sequences_started, 0,
            "busy cinematics must not restart or steal a lease"
        );
    }
    let _half = tick(&mut world, 0.5, &floor);
    let halfway = world.cinematic_view().ok_or("missing halfway view")?;
    assert!(
        (halfway.position.x - 1.0).abs() < 0.001,
        "camera must interpolate from the authored start"
    );
    let _paused = tick(&mut world, 0.0, &floor);
    world.refresh_cinematic_view();
    assert_eq!(
        world.cinematic_view(),
        Some(halfway),
        "pause/render sampling must not advance time"
    );
    assert!(
        world.checkpoint().is_err(),
        "an active cinematic is not a resumable checkpoint"
    );
    world.cancel_cinematic();
    assert!(
        world.cinematic_token().is_none() && idle(&world, "first"),
        "cancel releases view and sequence ownership"
    );
    assert_eq!(
        tick(&mut world, 1.0, &floor).sequences_completed,
        0,
        "cancel emits no success"
    );
    start(&mut world, "first", "shot");
    assert_ne!(
        world.cinematic_token(),
        Some(token),
        "re-entry receives a fresh revision"
    );
    world.reset_runtime();
    assert!(
        world.cinematic_view().is_none(),
        "reset drops camera ownership"
    );
    Ok(())
}

#[test]
fn cinematic_finish_retries_full_queue_once_without_replaying_steps() -> Result<(), String> {
    use crate::engine::entities::events::{EventKind, EventQueue};
    let (mut world, floor) = fixture(CINEMATIC, "[]")?;
    start(&mut world, "first", "shot");
    let owner = world.handle_of("first").ok_or("missing owner")?;
    for _ in 0..EventQueue::MAX_QUEUED_EVENTS {
        assert!(
            world.emit(EventKind::Timer, owner, "filler", None),
            "bounded fixture fill fits exactly"
        );
    }
    let first = tick(&mut world, 1.0, &floor);
    assert_eq!(
        first.sequences_completed, 0,
        "full queue must defer success rather than lose it"
    );
    assert_eq!(
        world.sequence_count(),
        1,
        "only the finishing run retains bounded retry state"
    );
    assert!(
        world.cinematic_token().is_none(),
        "finishing must immediately release player controls"
    );
    assert!(
        world.checkpoint().is_err(),
        "pending terminal delivery is not checkpointable"
    );
    world.cancel_cinematic();
    let _stop = world.dispatch_on("first", &[ActionDef::StopSequence { target: None }]);
    let rejected = world.dispatch_on(
        "second",
        &[ActionDef::StartSequence {
            sequence: "shot".into(),
            target: None,
        }],
    );
    assert_eq!(
        rejected.sequences_started, 0,
        "pending completion still reserves admission"
    );
    assert_eq!(
        tick(&mut world, 0.0, &floor).sequences_completed,
        0,
        "pause must not retry delivery"
    );
    assert_eq!(
        tick(&mut world, 0.1, &floor).sequences_completed,
        1,
        "positive tick delivers preserved success"
    );
    assert_eq!(
        world.sequence_count(),
        0,
        "accepted completion removes the run before callbacks"
    );
    assert_eq!(
        tick(&mut world, 0.1, &floor).sequences_completed,
        0,
        "success cannot be emitted twice"
    );
    Ok(())
}

#[test]
fn cinematic_live_target_refresh_is_generation_safe() -> Result<(), String> {
    let (mut world, floor) = fixture(
        r#"[{"id":"shot","cinematic":{"start":{"position":[0,2,0],"yaw_degrees":0,"pitch_degrees":0}},"steps":[{"step":"camera","seconds":10,"motion":{"kind":"follow","target":{"kind":"entity","id":"second"},"offset":[0,2,0]},"aim":{"kind":"look_at","target":{"kind":"entity","id":"second"}}}]}]"#,
        "[]",
    )?;
    start(&mut world, "first", "shot");
    let _first = tick(&mut world, 0.1, &floor);
    let target = world.handle_of("second").ok_or("missing target")?;
    let pose = world
        .components_mut()
        .transforms
        .get_mut(target)
        .ok_or("missing target transform")?;
    pose.position.x = 3.0;
    world.refresh_cinematic_view();
    let view = world.cinematic_view().ok_or("missing follow camera")?;
    assert!(
        (view.position.x - 3.0).abs() < 0.001
            && view.yaw.is_finite()
            && (view.pitch.to_degrees() + 89.0).abs() < 0.001,
        "late sampling tracks transforms and vertical look-at stays finite"
    );
    world
        .components_mut()
        .renderables
        .get_mut(target)
        .ok_or("missing target renderable")?
        .dynamic = true;
    let despawn = world.dispatch_on(
        "first",
        &[ActionDef::DespawnEntity {
            target: "second".into(),
        }],
    );
    assert_eq!(despawn.despawned, 1, "fixture target removal must succeed");
    world.refresh_cinematic_view();
    assert!(
        world.cinematic_token().is_none(),
        "missing generational target cancels before frame publication"
    );
    assert_eq!(
        tick(&mut world, 0.1, &floor).sequences_completed,
        0,
        "target loss is not success"
    );
    Ok(())
}

// Numeric/schema admission is cheap serde plus bounded iteration. It protects
// malformed authored camera data and old-map roundtrips, independently of the
// runtime lifecycle tests above.
#[test]
fn cinematic_admission_bounds_and_optional_serialization() -> Result<(), String> {
    use crate::engine::entities::cinematics::validate_cinematic;
    use crate::engine::entities::sequences::{SequenceDef, SequenceStepDef};
    let definitions: Vec<SequenceDef> =
        serde_json::from_str(CINEMATIC).map_err(|error| error.to_string())?;
    let definition = definitions.first().ok_or("missing fixture definition")?;
    validate_cinematic(definition)?;
    for duration in [0.0, -1.0, 120.1, f32::NAN, f32::INFINITY] {
        let mut invalid = definition.clone();
        if let Some(SequenceStepDef::Camera {
            seconds,
            motion: _,
            aim: _,
        }) = invalid.steps.first_mut()
        {
            *seconds = duration;
        }
        assert!(
            validate_cinematic(&invalid).is_err(),
            "invalid camera duration must refuse: {duration}"
        );
    }
    let mut missing = definition.clone();
    missing.cinematic = None;
    assert!(
        validate_cinematic(&missing).is_err(),
        "camera steps cannot silently acquire a gameplay camera"
    );
    let mut looped = definition.clone();
    looped.looped = true;
    assert!(
        validate_cinematic(&looped).is_err(),
        "cinematic cannot loop forever"
    );
    let ordinary: SequenceDef =
        serde_json::from_str(r#"{"id":"ordinary","steps":[{"step":"wait","seconds":1}]}"#)
            .map_err(|error| error.to_string())?;
    assert!(
        !serde_json::to_string(&ordinary)
            .map_err(|error| error.to_string())?
            .contains("cinematic"),
        "old maps retain absent optional metadata"
    );
    Ok(())
}

#[test]
fn stay_supersedes_a_pending_return_only_after_successful_move() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"stay","name":"Stay",
        "spawn":{"x":4.5,"z":2.5},"rooms":[{"width":5,"depth":3}],
        "props":[{"id":"actor","model":"core:rubber_duck","x":0.5,"z":1.5,
            "components":[{"component":"nav_agent","radius":0.3,"speed_mps":1},{"component":"ai"}]}],
        "sequences":[
            {"id":"depart","steps":[{"step":"move","x":1.5,"z":1.5,"speed":2}]},
            {"id":"here","after_move":"stay","steps":[{"step":"move","x":1.5,"z":1.5,"speed":2}]},
            {"id":"blocked","after_move":"stay","steps":[{"step":"move","x":2.5,"z":1.5,"speed":2}]},
            {"id":"skipped","after_move":"stay","steps":[{"step":"stop"},{"step":"move","x":2.5,"z":1.5,"speed":2}]}]
    }"#).map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let nav = crate::engine::nav::NavMesh::from_record(super::navigation_ai::grid(
        5,
        3,
        &(0..15).collect::<Vec<_>>(),
    ))?;
    let home = Vec3::new(0.5, 0.0, 1.5);
    let post = Vec3::new(1.5, 0.0, 1.5);
    for next in ["here", "blocked", "skipped"] {
        let mut world = EntityWorld::from_level(&level);
        start(&mut world, "actor", "depart");
        for _ in 0_u8..25 {
            let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            if world.sequence_count() == 0 {
                break;
            }
        }
        let returner = world.ai().agent("actor").ok_or("missing returner")?;
        assert!(
            matches!(returner.state, crate::engine::ai::AiState::Return { destination } if destination == home),
            "the older return must already be active"
        );
        assert!(
            returner.position.distance(post) < 0.02,
            "the new move starts at its goal"
        );

        if next == "blocked" {
            world.set_navigation_obstacles(std::iter::once(
                crate::engine::collision::WallAabb::with_y(2.0, 0.0, 0.0, 0.1, 2.9, 3.0),
            ));
        }
        start(&mut world, "actor", next);
        let mut furthest_x = post.x;
        let mut completions = 0_usize;
        for _ in 0_u8..70 {
            let tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            completions = completions.saturating_add(tick.sequences_completed);
            let actor = world.ai().agent("actor").ok_or("missing actor")?;
            furthest_x = furthest_x.max(actor.position.x);
        }
        let actor = world.ai().agent("actor").ok_or("missing actor")?;
        if next == "blocked" {
            assert!(
                furthest_x > 1.55,
                "the failed move must first displace the actor"
            );
            assert_eq!(completions, 0, "collision must cancel the sequence");
        }
        let expected = if next == "here" { post } else { home };
        assert!(
            actor.position.distance(expected) < 0.06,
            "{next} must leave the actor at {expected:?}, found {:?}",
            actor.position
        );
    }
    Ok(())
}

// Sequences share the same public collision handoff as normal play. A tiny
// floor with one runtime barrier distinguishes absence, explicit return, and
// stay, plus the failure mode of resetting the transform directly to home.
#[test]
fn scripted_npc_completion_returns_by_navigation_or_stays() -> Result<(), String> {
    let level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"post","name":"Post",
        "spawn":{"x":4.5,"z":2.5},"rooms":[{"width":5,"depth":3}],
        "props":[{"id":"actor","model":"core:rubber_duck","x":0.5,"z":1.5,
            "components":[{"component":"nav_agent","radius":0.3,"speed_mps":1},
                {"component":"ai","walk_speed":1.2}]}],
        "sequences":[
            {"id":"default","steps":[{"step":"move","x":1.5,"z":1.5,"speed":2}]},
            {"id":"return","after_move":"return_home","steps":[{"step":"move","x":1.5,"z":1.5,"speed":2}]},
            {"id":"stay","after_move":"stay","steps":[{"step":"move","x":2.5,"z":1.5,"speed":2}]}]
    }"#).map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let cells = (0_usize..15).collect::<Vec<_>>();
    let nav = crate::engine::nav::NavMesh::from_record(super::navigation_ai::grid(5, 3, &cells))?;
    let home = Vec3::new(0.5, 0.0, 1.5);
    for sequence in ["default", "return", "stay", "supersede"] {
        let mut world = EntityWorld::from_level(&level);
        let first = if sequence == "supersede" {
            "default"
        } else {
            sequence
        };
        start(&mut world, "actor", first);
        for _ in 0_u8..25 {
            let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            // Install an obstruction after scripted ownership is released.
            if world.sequence_count() == 0 {
                break;
            }
        }
        if sequence == "supersede" {
            let actor = world.ai().agent("actor").ok_or("missing returner")?;
            assert!(
                matches!(actor.state, crate::engine::ai::AiState::Return { destination } if destination == home)
            );
            start(&mut world, "actor", "stay");
        }
        if matches!(sequence, "stay" | "supersede") {
            for _ in 0_u8..48 {
                let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            }
            let actor = world.ai().agent("actor").ok_or("missing actor")?;
            assert!(actor.position.distance(Vec3::new(2.5, 0.0, 1.5)) < 0.03);
            assert_eq!(actor.home, home, "stay must preserve the authored post");
            assert!(world.set_ai_navigation_goal("actor", Vec3::new(3.5, 0.0, 1.5)));
            for _ in 0_u8..48 {
                let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            }
            assert!(world.ai().agent("actor").ok_or("missing actor")?.position.x > 3.0);
        } else {
            assert_eq!(world.sequence_count(), 0, "the scripted walk must finish");
            assert!(world.ai().agent("actor").ok_or("missing actor")?.position.x > 1.3);
            let blocker =
                crate::engine::collision::WallAabb::with_y(0.94, 0.0, 0.0, 0.05, 2.9, 3.0);
            world.set_navigation_obstacles(std::iter::once(blocker));
            for _ in 0_u8..48 {
                let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
                let actor = world
                    .ai()
                    .agent("actor")
                    .ok_or("missing blocked returner")?;
                assert!(
                    actor.position.x >= 0.99 + actor.profile.radius - 0.001,
                    "the return may not pass through the solid"
                );
            }
            world.set_navigation_obstacles(std::iter::empty());
            for _ in 0_u8..60 {
                let _tick = tick_with_nav(&mut world, 0.05, &floor, Some(&nav));
            }
            let actor = world.ai().agent("actor").ok_or("missing returner")?;
            assert!(
                actor.position.distance(home) < 0.06,
                "{sequence} must walk back after clearance: {:?}",
                actor.position
            );
        }
    }
    Ok(())
}

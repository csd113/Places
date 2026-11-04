//! Essential small CPU regressions for transaction identities, interrupted
//! continuations and perception fairness. No assets, map baking or native GPU.
use crate::engine::{
    ai::{
        AiBehavior, AiDef, AiOutcome, AiTarget, AiTickContext, AiWorld,
        authoring::{
            ItemSelector, NoiseKind, SenseFilter, SenseResponse, SenseRule, TaskDef, TaskRequest,
            UseKind, UsePoint,
        },
        tasks::AiObject,
    },
    collision::WallAabb,
    collision_index::CollisionIndex,
    door::Doors,
    entities::id::EntityHandle,
    level::{WalkableCeiling, WalkableFloor},
    nav::NavAgentProfile,
};
use glam::Vec3;

struct Fixture {
    world: AiWorld,
    time: f32,
    acks: Vec<crate::engine::ai::tasks::UseAck>,
    objects: Vec<AiObject>,
    targets: Vec<AiTarget>,
    noises: Vec<crate::engine::ai::AiStimulus>,
    walls: Vec<WallAabb>,
    obstacles: Vec<WallAabb>,
    index: CollisionIndex,
    doors: Doors,
    floor: WalkableFloor,
    ceiling: WalkableCeiling,
}
impl Fixture {
    fn new() -> Self {
        Self {
            world: AiWorld::new(),
            time: 0.0,
            acks: Vec::new(),
            objects: Vec::new(),
            targets: Vec::new(),
            noises: Vec::new(),
            walls: Vec::new(),
            obstacles: Vec::new(),
            index: CollisionIndex::empty(),
            doors: Doors::default(),
            floor: WalkableFloor::default(),
            ceiling: WalkableCeiling::default(),
        }
    }
    fn add(&mut self, id: &str, slot: u32, position: Vec3, def: AiDef) -> EntityHandle {
        let handle = EntityHandle::from_parts(slot, 0);
        self.world.register(
            handle,
            id,
            def,
            NavAgentProfile::reference(),
            position,
            0.0,
            false,
        );
        handle
    }
    fn tick(&mut self, seconds: f32, scripted: &[EntityHandle]) -> AiOutcome {
        self.time += seconds;
        let mut out = AiOutcome::default();
        self.world.tick(
            &AiTickContext {
                delta: seconds,
                sim_time: self.time,
                nav: None,
                doors: &self.doors,
                door_version: 0,
                walls: &self.walls,
                leaves: &[],
                index: &self.index,
                floor: &self.floor,
                ceiling: &self.ceiling,
                targets: &self.targets,
                stimuli: &self.noises,
                scripted,
                obstacles: &self.obstacles,
                objects: &self.objects,
                player_visible: &[],
                use_acks: &self.acks,
            },
            &mut out,
        );
        for noise in &mut self.noises {
            noise.age += seconds;
        }
        out
    }
    fn object(&mut self, id: &str, slot: u32, position: Vec3, loose: bool) {
        self.objects.push(AiObject {
            handle: EntityHandle::from_parts(slot, 0),
            id: id.to_string(),
            object_type: "parcel".to_string(),
            position,
            loose,
            radius: 0.1,
            carried_by: None,
        });
    }
}
fn request(key: &str, priority: u8, definition: TaskDef) -> TaskRequest {
    TaskRequest {
        key: key.to_string(),
        priority,
        definition,
    }
}
fn fetch() -> TaskDef {
    TaskDef::Fetch {
        item: ItemSelector::Id("parcel".into()),
        destination: UsePoint {
            target: "shelf".into(),
            approach: [0.0, 0.0, 3.0],
            contact: [0.0, 1.0, 3.0],
            yaw_degrees: None,
            reach: 1.5,
            kind: UseKind::Shelf,
            contact_seconds: 0.1,
            hold_seconds: 0.0,
            clip: None,
            socket: None,
        },
        search_radius: 8.0,
    }
}

#[test]
fn competing_fetch_cancellation_and_player_theft_invalidate_exact_operations() -> Result<(), String>
{
    let mut f = Fixture::new();
    let _actor_3812 = f.add("a", 1, Vec3::ZERO, AiDef::default());
    let _actor_3874 = f.add("b", 2, Vec3::X, AiDef::default());
    f.object("parcel", 10, Vec3::new(0.0, 0.0, 1.0), true);
    f.object("shelf", 11, Vec3::new(0.0, 0.0, 3.0), false);
    assert!(f.world.request_task("a", request("delivery", 80, fetch())));
    assert!(f.world.request_task("b", request("delivery", 80, fetch())));
    let _first = f.tick(0.1, &[]);
    let reserved = EntityHandle::from_parts(10, 0);
    let owners: Vec<_> = f
        .world
        .agents()
        .iter()
        .filter_map(|agent| agent.controller.active)
        .filter(|token| f.world.item_reserved(reserved, *token))
        .collect();
    assert_eq!(
        owners.len(),
        1,
        "one physical item admits one NPC reservation"
    );
    let token = *owners.first().ok_or("no reservation")?;
    let owner = if f.world.agent("a").is_some_and(|a| a.handle == token.owner) {
        "a"
    } else {
        "b"
    };
    assert!(f.world.cancel_task(owner, "delivery"));
    assert!(
        !f.world.operation_valid(token),
        "queued hand contact cannot survive cancellation"
    );
    let _recovery = f.tick(0.25, &[]);
    assert_eq!(
        f.world
            .agents()
            .iter()
            .filter_map(|agent| agent.controller.active)
            .filter(|current| f.world.item_reserved(reserved, *current))
            .count(),
        1,
        "another NPC recovers the cancelled reservation"
    );
    if let Some(item) = f.objects.first_mut() {
        item.loose = false;
    }
    let _tick_5197 = f.tick(0.25, &[]);
    assert!(
        !f.world
            .item_reserved(EntityHandle::from_parts(10, 0), token)
    );
    f.world.clear();
    let _actor_5346 = f.add("a", 1, Vec3::ZERO, AiDef::default());
    assert!(
        !f.world.operation_valid(token),
        "a reused entity slot cannot validate an old operation"
    );
    Ok(())
}

#[test]
fn routine_waits_pause_for_priority_sequence_and_light_then_resume() -> Result<(), String> {
    let mut f = Fixture::new();
    let actor = f.add("actor", 1, Vec3::ZERO, AiDef::default());
    assert!(f.world.request_task(
        "actor",
        request(
            "routine",
            10,
            TaskDef::Routine {
                steps: vec![
                    TaskDef::Wait { seconds: 1.0 },
                    TaskDef::Move {
                        point: [0.0, 0.0, 3.0],
                        speed: None
                    }
                ],
                looped: false
            }
        )
    ));
    let _tick_6202 = f.tick(0.25, &[]);
    let _tick_6236 = f.tick(0.5, &[actor]);
    assert!(f.world.set_light_response(actor, Some(Vec3::X)));
    let _tick_6337 = f.tick(0.5, &[]);
    assert!(f.world.set_light_response(actor, None));
    assert!(f.world.request_task(
        "actor",
        request("urgent", 200, TaskDef::Wait { seconds: 0.5 })
    ));
    let _tick_6546 = f.tick(0.5, &[]);
    let _tick_6579 = f.tick(0.5, &[]);
    let agent = f.world.agent("actor").ok_or("agent")?;
    let run = agent.controller.runs.first().ok_or("routine")?;
    assert_eq!(
        run.progress.index, 0,
        "interrupted wait does not consume hidden elapsed time"
    );
    let _tick_6849 = f.tick(0.3, &[]);
    let _tick_6882 = f.tick(0.1, &[]);
    assert_eq!(
        f.world.agent("actor").and_then(|a| a.navigation_goal),
        Some(Vec3::new(0.0, 0.0, 3.0)),
        "continuation reaches its next shared move"
    );
    assert!(f.world.mark_caught(actor));
    assert!(
        f.world
            .agent("actor")
            .is_some_and(|a| a.controller.runs.is_empty())
    );
    Ok(())
}

#[test]
fn lost_follower_and_predator_never_read_hidden_live_target_positions() -> Result<(), String> {
    for behavior in [AiBehavior::Follower, AiBehavior::Predator] {
        let mut f = Fixture::new();
        let _actor_7468 = f.add(
            "actor",
            1,
            Vec3::ZERO,
            AiDef {
                behavior,
                reacts_to: vec!["player".into()],
                sight_fov_degrees: 360.0,
                ..AiDef::default()
            },
        );
        let target = EntityHandle::from_parts(20, 0);
        f.targets.push(AiTarget {
            handle: target,
            instance_id: "player".into(),
            role: Some("player".into()),
            behavior: AiBehavior::Idler,
            position: Vec3::new(0.0, 0.0, 3.0),
            radius: 0.3,
            height: 1.8,
            caught: false,
        });
        let _tick_8152 = f.tick(0.25, &[]);
        if let Some(player) = f.targets.first_mut() {
            player.position = Vec3::new(99.0, 0.0, 99.0);
        }
        let _tick_8312 = f.tick(0.25, &[]);
        let agent = f.world.agent("actor").ok_or("agent")?;
        assert!(
            agent
                .path_goal
                .is_none_or(|goal| goal.x < 10.0 && goal.z < 10.0)
        );
        assert!(
            agent
                .navigation_goal
                .is_none_or(|goal| goal.x < 10.0 && goal.z < 10.0)
        );
        assert!(
            agent
                .last_known
                .as_ref()
                .is_some_and(|known| known.position == Vec3::new(0.0, 0.0, 3.0)),
            "memory remains at the actual sighting"
        );
    }
    Ok(())
}

#[test]
fn typed_hearing_filters_and_alert_hops_do_not_form_a_feedback_loop() {
    let mut f = Fixture::new();
    let _actor_9038 = f.add(
        "source",
        1,
        Vec3::ZERO,
        AiDef {
            senses: vec![SenseRule {
                filter: SenseFilter::Hear {
                    categories: vec![NoiseKind::Door],
                    radius: 3.0,
                },
                response: SenseResponse::Alert {
                    radius: 2.1,
                    hops: 2,
                },
            }],
            receive_alerts: true,
            ..AiDef::default()
        },
    );
    for slot in 2_u16..=4 {
        let _actor_9576 = f.add(
            &format!("receiver_{slot}"),
            u32::from(slot),
            Vec3::new(f32::from(slot.saturating_sub(1)) * 2.0, 0.0, 0.0),
            AiDef {
                receive_alerts: true,
                ..AiDef::default()
            },
        );
    }
    f.noises.push(crate::engine::ai::AiStimulus {
        position: Vec3::Z,
        radius: 8.0,
        loudness: 1.0,
        category: "movement".into(),
        source: None,
        age: 0.0,
    });
    let _tick_10105 = f.tick(0.25, &[]);
    assert!(
        f.world
            .agent("receiver_2")
            .is_some_and(|a| a.controller.runs.is_empty()),
        "unselected movement is ignored"
    );
    if let Some(noise) = f.noises.first_mut() {
        noise.category = "door".into();
        noise.age = 0.0;
    }
    for _ in 0_u8..8 {
        let _tick_10452 = f.tick(0.25, &[]);
    }
    assert!(
        f.world
            .agent("receiver_2")
            .is_some_and(|a| a.controller.runs.len() == 1)
    );
    assert!(
        f.world
            .agent("receiver_3")
            .is_some_and(|a| a.controller.runs.len() == 1)
    );
    assert!(
        f.world
            .agent("receiver_4")
            .is_some_and(|a| a.controller.runs.is_empty()),
        "two hops cannot propagate to the third receiver"
    );
}

#[test]
fn shared_authoring_rejects_unbounded_and_malformed_behaviours() -> Result<(), String> {
    let good = request("delivery", 80, fetch());
    assert!(good.is_valid());
    let encoded = serde_json::to_string(&good).map_err(|e| e.to_string())?;
    assert_eq!(
        serde_json::from_str::<TaskRequest>(&encoded).map_err(|e| e.to_string())?,
        good
    );
    let mut nested = TaskDef::Wait { seconds: 1.0 };
    for _ in 0_u8..5 {
        nested = TaskDef::Routine {
            steps: vec![nested],
            looped: false,
        };
    }
    assert!(!nested.is_valid());
    assert!(!request("bad", 201, TaskDef::Wait { seconds: f32::NAN }).is_valid());
    assert!(
        serde_json::from_str::<TaskDef>(r#"{"task":"wait","seconds":1,"omniscient":true}"#)
            .is_err()
    );
    Ok(())
}

// A paused live task must hold position instead of returning to its baseline
// post. This also guards pending native operations during light interruption.
#[test]
fn explicit_pause_holds_intent_and_light_disables_side_effects() -> Result<(), String> {
    let mut fixture = Fixture::new();
    let handle = fixture.add("actor", 1, Vec3::ZERO, AiDef::default());
    assert!(fixture.world.request_task(
        "actor",
        request(
            "routine",
            80,
            TaskDef::Move {
                point: [0.0, 0.0, 3.0],
                speed: None
            }
        )
    ));
    let _first = fixture.tick(0.25, &[]);
    let token = fixture
        .world
        .agent("actor")
        .and_then(|agent| agent.controller.active)
        .ok_or("token")?;
    assert!(fixture.world.operation_running(token));
    assert!(fixture.world.set_light_response(handle, Some(Vec3::X)));
    let _held = fixture.tick(0.25, &[]);
    assert!(fixture.world.operation_valid(token));
    assert!(
        !fixture.world.operation_running(token),
        "a light hold keeps custody but cannot commit native contact"
    );
    assert!(fixture.world.set_light_response(handle, None));
    assert!(fixture.world.pause_task("actor", "routine", true));
    let _paused = fixture.tick(0.5, &[]);
    let paused = fixture.world.agent("actor").ok_or("actor")?;
    assert!(paused.navigation_goal.is_none());
    assert_eq!(paused.position, Vec3::ZERO);
    assert!(!fixture.world.operation_valid(token));
    assert!(fixture.world.pause_task("actor", "routine", false));
    let _resumed = fixture.tick(0.25, &[]);
    assert_eq!(
        fixture
            .world
            .agent("actor")
            .and_then(|agent| agent.navigation_goal),
        Some(Vec3::new(0.0, 0.0, 3.0))
    );
    Ok(())
}

// Authored Beam previously advanced immediately when already at its post.
// This costs one metadata-only tick and protects its .8-second cadence.
#[test]
fn patrol_first_decision_has_no_extra_authored_wait() {
    let mut fixture = Fixture::new();
    let _actor = fixture.add("actor", 1, Vec3::ZERO, AiDef::default());
    assert!(fixture.world.request_task(
        "actor",
        request(
            "patrol",
            80,
            TaskDef::Patrol {
                points: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 3.0]],
                speed: 1.0,
                memory_seconds: 6.0,
                track_player: false,
                decision_seconds: 0.8
            }
        )
    ));
    let _tick = fixture.tick(0.05, &[]);
    assert_eq!(
        fixture
            .world
            .agent("actor")
            .and_then(|agent| agent.navigation_goal),
        Some(Vec3::new(0.0, 0.0, 3.0))
    );
}

// A real delivered parcel settled into the actor clearance disc and trapped it.
// One tiny sweep fixture guards outward recovery and rejects deeper penetration;
// static collision stays strict. No assets, timing, native rendering or baking.
#[test]
fn shared_ai_dropped_body_overlap_can_escape_without_crossing_solids() {
    use crate::engine::collision::{disc_segment_blocked, dynamic_disc_segment_blocked};
    use glam::Vec2;
    let solids = [WallAabb::with_y(0.0, 0.0, 0.0, 1.0, 1.0, 1.0)];
    let from = Vec2::new(-0.2, 0.5);
    assert!(!dynamic_disc_segment_blocked(
        from,
        Vec2::new(-0.5, 0.5),
        0.4,
        0.0,
        1.8,
        &solids
    ));
    assert!(dynamic_disc_segment_blocked(
        from,
        Vec2::new(0.2, 0.5),
        0.4,
        0.0,
        1.8,
        &solids
    ));
    assert!(dynamic_disc_segment_blocked(
        Vec2::new(-0.5, 0.5),
        Vec2::new(1.5, 0.5),
        0.4,
        0.0,
        1.8,
        &solids
    ));
    assert!(disc_segment_blocked(
        from,
        Vec2::new(-0.5, 0.5),
        0.4,
        0.0,
        1.8,
        0.0,
        &CollisionIndex::empty(),
        &solids
    ));
}

// Capturing only AiState discarded an authored navigation speed/goal. This
// tiny metadata fixture also protects cancellation before first activation;
// no asset, navigation bake, GPU or wall-clock dependency is involved.
#[test]
fn shared_ai_preemption_restores_authored_navigation_and_queued_cancel_is_inert()
-> Result<(), String> {
    let mut fixture = Fixture::new();
    let _actor = fixture.add("actor", 1, Vec3::ZERO, AiDef::default());
    let destination = Vec3::new(0.0, 0.0, 4.0);
    assert!(
        fixture
            .world
            .set_navigation_goal_with_speed("actor", destination, Some(0.6))
    );
    assert!(fixture.world.request_task(
        "actor",
        request("queued", 80, TaskDef::Wait { seconds: 1.0 })
    ));
    assert!(fixture.world.cancel_task("actor", "queued"));
    assert_eq!(
        fixture.world.agent("actor").ok_or("actor")?.navigation_goal,
        Some(destination)
    );
    assert!(fixture.world.request_task(
        "actor",
        request("active", 80, TaskDef::Wait { seconds: 0.1 })
    ));
    let _active = fixture.tick(0.05, &[]);
    assert!(
        fixture
            .world
            .agent("actor")
            .ok_or("actor")?
            .navigation_goal
            .is_none()
    );
    let _complete = fixture.tick(0.1, &[]);
    let actor = fixture.world.agent("actor").ok_or("actor")?;
    assert_eq!(actor.navigation_goal, Some(destination));
    assert_eq!(actor.navigation_speed, Some(0.6));
    Ok(())
}

// A reserved item moved out of sight previously steered fetch using its live
// transform. Two metadata-only ticks protect the same observed-position rule
// as follow/predator; this fixture needs no assets, baking or native process.
#[test]
fn shared_ai_fetch_keeps_sampled_item_position_after_hidden_movement() -> Result<(), String> {
    let mut fixture = Fixture::new();
    let _actor = fixture.add("actor", 1, Vec3::ZERO, AiDef::default());
    fixture.object("parcel", 10, Vec3::Z, true);
    fixture.object("shelf", 11, Vec3::new(0.0, 0.0, 3.0), false);
    assert!(
        fixture
            .world
            .request_task("actor", request("delivery", 80, fetch()))
    );
    let _seen = fixture.tick(0.1, &[]);
    fixture.objects.first_mut().ok_or("item")?.position = Vec3::new(20.0, 0.0, 5.0);
    let _hidden = fixture.tick(0.25, &[]);
    let actor = fixture.world.agent("actor").ok_or("actor")?;
    assert_eq!(
        actor
            .controller
            .runs
            .first()
            .ok_or("task")?
            .progress
            .last_known,
        Some(Vec3::Z)
    );
    assert!(
        actor
            .navigation_goal
            .is_some_and(|point| point.x.abs() < 0.01)
    );
    Ok(())
}

// Luau already checked moving physical bounds while shared sight omitted them.
// A few metadata-only ticks protect baseline/task consistency, hidden movement
// and target-body exclusion with tiny cuboids; no assets, bake or GPU required.
#[test]
fn shared_ai_live_solids_occlude_actors_without_hiding_the_target_item() -> Result<(), String> {
    let mut f = Fixture::new();
    let _actor = f.add(
        "actor",
        1,
        Vec3::ZERO,
        AiDef {
            behavior: AiBehavior::Predator,
            reacts_to: vec!["player".into()],
            sight_fov_degrees: 360.0,
            ..AiDef::default()
        },
    );
    f.targets.push(AiTarget {
        handle: EntityHandle::from_parts(20, 0),
        instance_id: "player".into(),
        role: Some("player".into()),
        behavior: AiBehavior::Idler,
        position: Vec3::new(0.0, 0.0, 4.0),
        radius: 0.3,
        height: 1.8,
        caught: false,
    });
    let blocker = WallAabb::with_y(-1.0, 0.0, 1.0, 2.0, 2.0, 1.0);
    f.obstacles.push(blocker);
    assert!(f.world.request_task(
        "actor",
        request(
            "follow",
            80,
            TaskDef::Follow {
                target: "player".into(),
                distance: 1.0,
                memory_seconds: 4.0
            }
        )
    ));
    let _blocked = f.tick(0.25, &[]);
    let actor = f.world.agent("actor").ok_or("actor")?;
    assert!(actor.last_known.is_none());
    assert!(
        actor
            .controller
            .runs
            .first()
            .ok_or("task")?
            .progress
            .last_known
            .is_none()
    );
    f.obstacles.clear();
    let _seen = f.tick(0.25, &[]);
    f.obstacles.push(blocker);
    f.targets.first_mut().ok_or("player")?.position = Vec3::new(1.0, 0.0, 4.0);
    let _hidden = f.tick(0.25, &[]);
    let hidden_actor = f.world.agent("actor").ok_or("actor")?;
    assert_eq!(
        hidden_actor
            .last_known
            .as_ref()
            .ok_or("baseline sight")?
            .position,
        Vec3::new(0.0, 0.0, 4.0)
    );
    assert_eq!(
        hidden_actor
            .controller
            .runs
            .first()
            .ok_or("task")?
            .progress
            .last_known,
        Some(Vec3::new(0.0, 0.0, 4.0))
    );
    let mut item = Fixture::new();
    let _owner = item.add("actor", 1, Vec3::ZERO, AiDef::default());
    item.object("parcel", 10, Vec3::Z, true);
    item.object("shelf", 11, Vec3::new(0.0, 0.0, 3.0), false);
    item.obstacles
        .push(WallAabb::with_y(-0.1, 0.0, 0.9, 0.2, 0.2, 0.2));
    assert!(
        item.world
            .request_task("actor", request("fetch", 80, fetch()))
    );
    let _pickup = item.tick(0.25, &[]);
    assert!(
        item.world
            .agent("actor")
            .ok_or("owner")?
            .controller
            .runs
            .first()
            .ok_or("fetch")?
            .progress
            .item
            .is_some()
    );
    Ok(())
}

// A native delivery restored the real parcel, then treated it as an unrelated
// occluder and failed its own contact. Four tiny metadata ticks protect that
// boundary and prove a separate solid still blocks it; no assets/bake/GPU.
#[test]
fn released_delivery_body_does_not_occlude_its_own_contact() -> Result<(), String> {
    let mut f = Fixture::new();
    let owner = f.add("actor", 1, Vec3::new(0.0, 0.0, 3.0), AiDef::default());
    f.object("parcel", 10, Vec3::new(0.0, 0.15, 3.3), true);
    f.object("shelf", 11, Vec3::new(0.0, 0.0, 3.0), false);
    let mut delivery = fetch();
    if let TaskDef::Fetch {
        destination,
        item: _,
        search_radius: _,
    } = &mut delivery
    {
        destination.contact = [0.0, 0.24, 3.27];
    }
    assert!(
        f.world
            .request_task("actor", request("delivery", 80, delivery))
    );
    let _pickup = f.tick(0.25, &[]);
    f.objects.first_mut().ok_or("parcel")?.carried_by = Some(owner);
    let _carried = f.tick(0.25, &[]);
    f.objects.first_mut().ok_or("parcel")?.carried_by = None;
    f.obstacles
        .push(WallAabb::with_y(-0.2, 0.0, 3.1, 0.4, 0.5, 0.4));
    let released = f.tick(0.25, &[]);
    assert!(
        released.task_effects.iter().any(|effect| matches!(
            effect,
            crate::engine::ai::tasks::TaskEffect::Use { token: _, point: _ }
        )),
        "released cargo must permit its destination contact"
    );
    f.obstacles
        .push(WallAabb::with_y(-0.2, 0.6, 3.0, 0.4, 0.3, 0.4));
    let _blocked = f.tick(0.25, &[]);
    assert_eq!(
        f.world
            .agent("actor")
            .ok_or("actor")?
            .controller
            .last_result
            .as_deref(),
        Some("delivery:interaction_out_of_reach")
    );
    Ok(())
}

// Native shelf release rotated the body so its carried render origin lay on
// the collider edge. One metadata-only world tick protects using the existing
// solver shape-centre path for AI, without changing script/render transforms.
#[test]
fn shared_ai_physical_observation_uses_shape_center_after_rotation() -> Result<(), String> {
    use crate::engine::entities::{EntityWorld, TriggerItem, WorldContext};
    let level=crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"physical_ai","name":"Physical AI","spawn":{"x":3,"z":3},"rooms":[{"width":4,"depth":4}],"props":[{"id":"actor","model":"mannequin","components":[{"component":"nav_agent","speed_mps":1,"radius":0.22,"height":1.8},{"component":"ai","sight_fov_degrees":360}]},{"id":"parcel","model":"core:cardboard_box","z":1.5},{"id":"shelf","model":"core:crate","z":3}]}"#).map_err(|e|e.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let origin = Vec3::new(0.0, 0.85, 1.503);
    let center = Vec3::new(0.0, 0.85, 1.641);
    let centers = [center];
    world.observe_physical_items(std::iter::once(TriggerItem {
        key: 10,
        instance: "parcel",
        asset: "core:cardboard_box",
        from: origin,
        to: origin,
        half_extents: Vec3::splat(0.1375),
        held: false,
        sleeping: false,
        up_y: 0.0,
        path: None,
        centre_path: Some(&centers),
    }));
    assert!(
        world
            .ai_mut()
            .request_task("actor", request("delivery", 80, fetch()))
    );
    let floor = WalkableFloor::from_level(&level);
    let index = CollisionIndex::empty();
    let _tick = world.tick(&WorldContext {
        delta_seconds: 0.25,
        feet_from: Vec3::splat(3.0),
        feet: Vec3::splat(3.0),
        eye: Vec3::splat(3.0),
        body_height: 1.8,
        body_half: 0.4,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    });
    assert_eq!(
        world
            .ai()
            .agent("actor")
            .ok_or("actor")?
            .controller
            .runs
            .first()
            .ok_or("delivery")?
            .progress
            .last_known,
        Some(center)
    );
    Ok(())
}

// Sixteen candidates exceed a sight batch. This protects persistent sight,
// real loss/reacquisition and several visible candidates at constant CPU cost.
#[test]
fn sight_batches_keep_visible_targets_until_actual_loss() -> Result<(), String> {
    let mut f = Fixture::new();
    let _observer = f.add(
        "observer",
        1,
        Vec3::ZERO,
        AiDef {
            sight_range: 0.0,
            hearing_range: 0.0,
            senses: vec![SenseRule {
                filter: SenseFilter::See {
                    player: false,
                    entities: true,
                    objects: vec![],
                    object_types: vec![],
                    range: 5.0,
                    fov_degrees: 360.0,
                },
                response: SenseResponse::Alert {
                    radius: 5.0,
                    hops: 1,
                },
            }],
            ..AiDef::default()
        },
    );
    for n in 0_u16..16 {
        f.targets.push(AiTarget {
            handle: EntityHandle::from_parts(u32::from(n).saturating_add(10), 0),
            instance_id: format!("target_{n}"),
            role: None,
            behavior: AiBehavior::Idler,
            position: if n == 0 {
                Vec3::Z
            } else {
                Vec3::new(100.0 + f32::from(n), 0.0, 0.0)
            },
            radius: 0.2,
            height: 1.0,
            caught: false,
        });
    }
    for _ in 0_u8..20 {
        let _out = f.tick(0.21, &[]);
    }
    assert_eq!(
        f.world
            .resource_snapshot()
            .get("alerts")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    f.targets.first_mut().ok_or("target")?.position = Vec3::Z * 100.0;
    for _ in 0_u8..3 {
        let _out = f.tick(0.21, &[]);
    }
    f.targets.first_mut().ok_or("target")?.position = Vec3::Z;
    for _ in 0_u8..3 {
        let _out = f.tick(0.21, &[]);
    }
    assert_eq!(
        f.world
            .resource_snapshot()
            .get("alerts")
            .and_then(serde_json::Value::as_u64),
        Some(2),
        "real loss permits one reacquisition alert"
    );
    // Several farther visible candidates must not evict the nearest remembered
    // actor as batches rotate. Old alert records expire during this phase.
    for (index, target) in f.targets.iter_mut().enumerate().skip(1) {
        let offset = f32::from(u16::try_from(index).map_err(|e| e.to_string())?) * 0.1;
        target.position = Vec3::new(0.0, 0.0, 2.0 + offset);
    }
    for _ in 0_u8..35 {
        let _out = f.tick(0.21, &[]);
    }
    assert_eq!(
        f.world
            .resource_snapshot()
            .get("alerts")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "stable visibility publishes no new alerts"
    );
    Ok(())
}

// A completed child shares the routine token with its successor. Two parcels
// protect the actual native-contact lookup, then a wait/second NPC proves that
// successful delivery releases ownership before the parent routine ends.
#[test]
fn routine_deliveries_release_each_parcel_before_the_next_step() -> Result<(), String> {
    let mut f = Fixture::new();
    let owner = f.add("a", 1, Vec3::ZERO, AiDef::default());
    f.object("parcel", 10, Vec3::new(0.0, 0.0, 0.6), true);
    f.object("shelf", 11, Vec3::Y, false);
    f.object("parcel2", 12, Vec3::new(0.0, 0.0, 0.6), true);
    f.object("shelf2", 13, Vec3::new(1.0, 1.0, 0.0), false);
    let first = nearby_fetch("parcel", "shelf", [0.0, 1.0, 0.0]);
    let second = nearby_fetch("parcel2", "shelf2", [1.0, 1.0, 0.0]);
    assert!(f.world.request_task(
        "a",
        request(
            "routine",
            80,
            TaskDef::Routine {
                steps: vec![first, second, TaskDef::Wait { seconds: 60.0 }],
                looped: false,
            }
        )
    ));
    let _start = f.tick(0.1, &[]);
    let token = f
        .world
        .agent("a")
        .and_then(|a| a.controller.active)
        .ok_or("token")?;
    for (index, slot, shelf) in [(0, 10, "shelf"), (2, 12, "shelf2")] {
        assert_eq!(
            f.world.reserved_item(token),
            Some(EntityHandle::from_parts(slot, 0)),
            "native contact resolves only this delivery's parcel"
        );
        let item = f.objects.get_mut(index).ok_or("parcel")?;
        item.carried_by = Some(owner);
        item.loose = false;
        let out = f.tick(0.1, &[]);
        assert!(out.task_effects.iter().any(|effect| matches!(
            effect,
            crate::engine::ai::tasks::TaskEffect::Putdown {
                token: _,
                contact: _
            }
        )));
        assert!(
            f.world
                .item_reserved(EntityHandle::from_parts(slot, 0), token),
            "custody survives until physical release and acknowledgement"
        );
        let released = f.objects.get_mut(index).ok_or("parcel")?;
        released.carried_by = None;
        released.loose = true;
        released.position = if slot == 10 {
            Vec3::Y
        } else {
            Vec3::new(1.0, 1.0, 0.0)
        };
        let contact = f.tick(0.2, &[]);
        assert!(contact.task_effects.iter().any(|effect| matches!(
            effect,
            crate::engine::ai::tasks::TaskEffect::Use { token: _, point: _ }
        )));
        f.acks.push(crate::engine::ai::tasks::UseAck {
            token,
            target: shelf.into(),
            completed_at: f.time,
        });
        let _complete = f.tick(0.1, &[]);
        assert!(
            !f.world
                .item_reserved(EntityHandle::from_parts(slot, 0), token)
        );
        let _next = f.tick(0.1, &[]);
    }
    assert_eq!(f.world.reserved_item(token), None);
    let run = f
        .world
        .agent("a")
        .and_then(|a| a.controller.runs.first())
        .ok_or("routine")?;
    assert_eq!(run.progress.child.as_ref().ok_or("wait")?.status, "waiting");
    let _second_owner = f.add("b", 2, Vec3::ZERO, AiDef::default());
    assert!(f.world.request_task("b", request("collect", 80, fetch())));
    let _collect = f.tick(0.21, &[]);
    let next = f
        .world
        .agent("b")
        .and_then(|a| a.controller.active)
        .ok_or("collector")?;
    assert!(f.world.item_reserved(EntityHandle::from_parts(10, 0), next));
    Ok(())
}

// Simulated seconds cost only 600 tiny ticks. Explicit waits finish at their
// authored duration while an unreachable move retains the generic stuck guard.
#[test]
fn long_waits_complete_but_stuck_moves_still_timeout() -> Result<(), String> {
    let mut f = Fixture::new();
    let _waiter = f.add("waiter", 1, Vec3::ZERO, AiDef::default());
    let _mover = f.add("mover", 2, Vec3::ZERO, AiDef::default());
    assert!(f.world.request_task(
        "waiter",
        request("long_wait", 80, TaskDef::Wait { seconds: 300.0 })
    ));
    assert!(f.world.request_task(
        "mover",
        request(
            "stuck",
            80,
            TaskDef::Move {
                point: [20.0, 0.0, 0.0],
                speed: None
            }
        )
    ));
    for _ in 0_u16..599 {
        let _out = f.tick(0.5, &[]);
    }
    let waiter = f.world.agent("waiter").ok_or("waiter")?;
    assert_eq!(waiter.controller.last_result, None);
    assert_eq!(waiter.controller.runs.len(), 1);
    assert_eq!(
        f.world
            .agent("mover")
            .ok_or("mover")?
            .controller
            .last_result
            .as_deref(),
        Some("stuck:timeout")
    );
    let _last = f.tick(0.5, &[]);
    let finished = f.world.agent("waiter").ok_or("waiter")?;
    assert_eq!(
        finished.controller.last_result.as_deref(),
        Some("long_wait:complete")
    );
    assert!(finished.controller.runs.is_empty());
    Ok(())
}

// Preempting a fetch must release its item, then re-observe and reserve on
// resumption. Cancellation invalidates that same token at native contact.
#[test]
fn preempted_fetch_reacquires_then_cancellation_releases_custody() -> Result<(), String> {
    let mut f = Fixture::new();
    let _actor = f.add("a", 1, Vec3::ZERO, AiDef::default());
    f.object("parcel", 10, Vec3::Z, true);
    f.object("shelf", 11, Vec3::Z * 3.0, false);
    assert!(f.world.request_task("a", request("fetch", 80, fetch())));
    let _initial = f.tick(0.1, &[]);
    let token = f
        .world
        .agent("a")
        .and_then(|a| a.controller.active)
        .ok_or("token")?;
    let item = EntityHandle::from_parts(10, 0);
    assert!(f.world.item_reserved(item, token));
    assert!(
        f.world
            .request_task("a", request("urgent", 100, TaskDef::Wait { seconds: 0.5 }))
    );
    let _preempt = f.tick(0.1, &[]);
    assert!(!f.world.item_reserved(item, token));
    for _ in 0_u8..5 {
        let _out = f.tick(0.1, &[]);
    }
    assert!(f.world.item_reserved(item, token));
    assert!(f.world.cancel_task("a", "fetch"));
    assert!(!f.world.native_contact_valid(token));
    assert_eq!(f.world.reserved_item(token), None);
    Ok(())
}

fn nearby_fetch(item_id: &str, shelf: &str, contact: [f32; 3]) -> TaskDef {
    let mut definition = fetch();
    if let TaskDef::Fetch {
        item,
        destination,
        search_radius: _,
    } = &mut definition
    {
        *item = ItemSelector::Id(item_id.into());
        destination.target = shelf.into();
        destination.approach = [0.0, 0.0, 0.0];
        destination.contact = contact;
    }
    definition
}

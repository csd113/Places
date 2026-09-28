//! AI framework tests: perception, behavior transitions, hearing, catching
//! and the animation bridge. These exercise [`AiWorld`] directly with the real
//! baked navigation of a generated level.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic
// here; the production lints stay enforced everywhere else.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic
)]

use super::*;
use crate::collision_index::CollisionIndex;
use crate::door::Doors;
use crate::entities::id::EntityStore;
use crate::game::CollisionWorld;
use crate::level::LevelDef;
use crate::nav::{NavBakeInput, NavBakeOptions, NavMesh, bake, reference_class};

/// Builds a room with a partial dividing wall; agents are registered directly.
fn fixture() -> (LevelDef, CollisionWorld, Doors) {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "ai_fixture", "name": "AI Fixture",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 24.0, "depth": 12.0, "height": 3.0 } ],
            "walls": [ { "x": 11.7, "z": 0.0, "width": 0.3, "depth": 6.0, "height": 3.0 } ]
        }"#,
    )
    .expect("the AI fixture parses");
    let collision = CollisionWorld::from_level(&level);
    let doors = Doors::from_level(&level);
    (level, collision, doors)
}

fn bake_mesh(level: &LevelDef, collision: &CollisionWorld, doors: &Doors) -> NavMesh {
    let classes = vec![
        reference_class(),
        crate::package::navigation::NavClass::new(0.1, 0.2, 0.2, crate::nav::NAV_MAX_SLOPE)
            .expect("rat class"),
        crate::package::navigation::NavClass::new(0.2, 0.5, 0.3, crate::nav::NAV_MAX_SLOPE)
            .expect("cat class"),
    ];
    let input = NavBakeInput {
        level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors,
        classes: &classes,
        obstacles: &[],
        walk_proxies: &[],
    };
    let (grid, _) = bake(&input, &NavBakeOptions::default()).expect("the fixture bakes");
    NavMesh::from_record(grid).expect("the mesh validates")
}

fn prey_def() -> AiDef {
    AiDef {
        behavior: AiBehavior::Prey,
        role: Some("prey_rat".to_string()),
        reacts_to: vec!["predator".to_string()],
        run_speed: 2.4,
        walk_speed: 0.8,
        sight_range: 10.0,
        sight_fov_degrees: 250.0,
        hearing_range: 12.0,
        flee_distance: 6.0,
        ..AiDef::default()
    }
}

fn predator_def() -> AiDef {
    AiDef {
        behavior: AiBehavior::Predator,
        role: Some("predator".to_string()),
        reacts_to: vec!["prey_rat".to_string()],
        run_speed: 3.0,
        walk_speed: 0.8,
        sight_range: 12.0,
        sight_fov_degrees: 240.0,
        hearing_range: 12.0,
        pursue_distance: 25.0,
        catch_radius: 0.5,
        catch_height: 0.7,
        ..AiDef::default()
    }
}

fn rat_body(can_open_doors: bool) -> NavAgentProfile {
    NavAgentProfile {
        radius: 0.1,
        height: 0.2,
        step_height: 0.2,
        max_slope: crate::nav::NAV_MAX_SLOPE,
        can_open_doors,
    }
}

fn cat_body(can_open_doors: bool) -> NavAgentProfile {
    NavAgentProfile {
        radius: 0.2,
        height: 0.5,
        step_height: 0.3,
        max_slope: crate::nav::NAV_MAX_SLOPE,
        can_open_doors,
    }
}

/// One test rig: world, collision and a target snapshot rebuilt per tick.
struct Rig {
    world: AiWorld,
    predator: EntityHandle,
    prey: EntityHandle,
    collision: CollisionWorld,
    doors: Doors,
    index: CollisionIndex,
    mesh: NavMesh,
}

impl Rig {
    fn new(prey_position: Vec3, predator_position: Vec3, prey_def: AiDef) -> Self {
        let (level, collision, doors) = fixture();
        let mesh = bake_mesh(&level, &collision, &doors);
        let mut store = EntityStore::new();
        let predator = store.insert();
        let prey = store.insert();
        let mut world = AiWorld::new();
        world.register(
            predator,
            "cat",
            predator_def(),
            cat_body(true),
            predator_position,
            0.0,
            false,
        );
        world.register(
            prey,
            "rat",
            prey_def,
            rat_body(false),
            prey_position,
            0.0,
            false,
        );
        let index = CollisionIndex::build(&collision.walls);
        Self {
            world,
            predator,
            prey,
            collision,
            doors,
            index,
            mesh,
        }
    }

    fn targets(&self) -> Vec<AiTarget> {
        self.world
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect()
    }

    fn tick(&mut self, ticks: usize) -> AiOutcome {
        let mut last = AiOutcome::default();
        for _ in 0..ticks {
            let targets = self.targets();
            let stimuli: Vec<Stimulus> = Vec::new();
            let ctx = AiTickContext {
                delta: 1.0 / 60.0,
                sim_time: 0.0,
                nav: Some(&self.mesh),
                doors: &self.doors,
                door_version: 0,
                walls: &self.collision.walls,
                index: &self.index,
                floor: &self.collision.floor,
                leaves: &[],
                targets: &targets,
                stimuli: &stimuli,
                scripted: &[],
            };
            let mut out = AiOutcome::default();
            self.world.tick(&ctx, &mut out);
            last = out;
        }
        last
    }
}

#[test]
fn sight_requires_range_fov_and_line_of_sight() {
    // Clear line of sight: the predator sees the rat and pursues.
    let mut rig = Rig::new(
        Vec3::new(14.0, 0.0, 8.0),
        Vec3::new(6.0, 0.0, 8.0),
        prey_def(),
    );
    rig.tick(1);
    let predator = rig.world.agent("cat").expect("the cat exists");
    assert!(
        matches!(predator.state, AiState::Pursue { .. }),
        "the cat pursues what it sees, got {:?}",
        predator.state
    );

    // The dividing wall (x = 11.7, z 0..6) blocks the same distance.
    let mut blocked = Rig::new(
        Vec3::new(14.0, 0.0, 3.0),
        Vec3::new(6.0, 0.0, 3.0),
        prey_def(),
    );
    blocked.tick(1);
    let predator = blocked.world.agent("cat").expect("the cat exists");
    assert!(
        !matches!(predator.state, AiState::Pursue { .. }),
        "a wall blocks sight, got {:?}",
        predator.state
    );

    // Out of range: no pursuit either.
    let mut far = Rig::new(
        Vec3::new(23.0, 0.0, 10.0),
        Vec3::new(0.5, 0.0, 10.0),
        prey_def(),
    );
    far.tick(1);
    let predator = far.world.agent("cat").expect("the cat exists");
    assert!(!matches!(predator.state, AiState::Pursue { .. }));
}

#[test]
fn a_fleeing_rat_moves_away_and_keeps_clear_of_the_wall() {
    let mut rig = Rig::new(
        Vec3::new(10.0, 0.0, 8.0),
        Vec3::new(5.0, 0.0, 8.0),
        prey_def(),
    );
    let start = rig.world.agent("rat").expect("rat").position;
    let mut saw_flee = false;
    let mut travelled = 0.0_f32;
    for _ in 0..240 {
        rig.tick(1);
        let rat = rig.world.agent("rat").expect("rat");
        if matches!(rat.state, AiState::Flee { .. }) {
            saw_flee = true;
        }
        travelled = travelled.max(rat.position.distance(start));
        // Never inside the dividing wall (x 11.7..12.0, z 0..6).
        assert!(
            !(rat.position.x > 11.55 && rat.position.x < 12.15 && rat.position.z < 6.3),
            "the rat entered the wall at {:?}",
            rat.position
        );
    }
    assert!(saw_flee, "the rat must flee the cat it sees");
    assert!(
        travelled > 0.5,
        "the rat must move, travelled {travelled} m"
    );
}

#[test]
fn a_catch_fires_once_and_freezes_the_prey() {
    let mut rig = Rig::new(
        Vec3::new(10.3, 0.0, 8.0),
        Vec3::new(10.0, 0.0, 8.0),
        prey_def(),
    );
    let outcome = rig.tick(1);
    assert_eq!(outcome.catches.len(), 1, "the adjacent catch fires once");
    let rat = rig.world.agent("rat").expect("rat");
    assert!(rat.caught, "the caught rat is frozen");
    assert!(matches!(rat.state, AiState::Caught));
    assert_eq!(rat.speed_mps, 0.0);
    // More ticks do not catch again or move the prey.
    let position = rat.position;
    let outcome = rig.tick(120);
    assert!(outcome.catches.is_empty(), "a catch fires once");
    let rat = rig.world.agent("rat").expect("rat");
    assert_eq!(rat.position, position, "a caught rat does not move");
}

#[test]
fn hearing_finds_a_threat_out_of_sight() {
    // Sight disabled entirely: only the stimulus can reach the rat.
    let mut def = prey_def();
    def.sight_range = 0.0;
    let mut rig = Rig::new(Vec3::new(10.0, 0.0, 8.0), Vec3::new(5.0, 0.0, 8.0), def);
    let predator = rig.predator;
    let rat = rig.prey;
    let predator_position = rig.world.agent("cat").expect("cat").position;
    // One stimulus tick with the cat as the source.
    let targets = rig.targets();
    let stimuli = vec![Stimulus {
        position: predator_position,
        radius: 8.0,
        loudness: 1.0,
        category: "movement".to_string(),
        source: Some(predator),
        age: 0.0,
    }];
    let ctx = AiTickContext {
        delta: 1.0 / 60.0,
        sim_time: 0.0,
        nav: Some(&rig.mesh),
        doors: &rig.doors,
        door_version: 0,
        walls: &rig.collision.walls,
        index: &rig.index,
        floor: &rig.collision.floor,
        leaves: &[],
        targets: &targets,
        stimuli: &stimuli,
        scripted: &[],
    };
    let mut out = AiOutcome::default();
    rig.world.tick(&ctx, &mut out);
    let rat_agent = rig.world.agent("rat").expect("rat");
    assert_eq!(rat_agent.handle, rat);
    assert!(
        matches!(rat_agent.state, AiState::Flee { .. }),
        "a heard threat starts a flee, got {:?}",
        rat_agent.state
    );
    // Silence and distance: the rat settles back to idle.
    rig.tick(300);
    let rat_agent = rig.world.agent("rat").expect("rat");
    assert!(
        !matches!(rat_agent.state, AiState::Flee { .. }),
        "the rat settles after the noise stops, got {:?}",
        rat_agent.state
    );
}

#[test]
fn the_animation_bridge_maps_states_to_gaits() {
    assert_eq!(
        movement::cue_for(AiState::Idle, 0.0),
        crate::entity::PoseCue::Idle
    );
    assert_eq!(
        movement::cue_for(AiState::Caught, 2.0),
        crate::entity::PoseCue::Idle
    );
    assert_eq!(
        movement::cue_for(
            AiState::Flee {
                threat: None,
                destination: Vec3::ZERO
            },
            1.4
        ),
        crate::entity::PoseCue::Walk { speed_mps: 1.4 }
    );
    assert_eq!(
        movement::cue_for(
            AiState::Pursue {
                target: EntityStore::new().insert()
            },
            1.9
        ),
        crate::entity::PoseCue::Walk { speed_mps: 1.9 }
    );
}

#[test]
fn a_scripted_agent_yields_locomotion() {
    let mut rig = Rig::new(
        Vec3::new(10.0, 0.0, 8.0),
        Vec3::new(5.0, 0.0, 8.0),
        prey_def(),
    );
    let rat = rig.prey;
    let targets = rig.targets();
    let stimuli: Vec<Stimulus> = Vec::new();
    let scripted = vec![rat];
    let ctx = AiTickContext {
        delta: 1.0 / 60.0,
        sim_time: 0.0,
        nav: Some(&rig.mesh),
        doors: &rig.doors,
        door_version: 0,
        walls: &rig.collision.walls,
        index: &rig.index,
        floor: &rig.collision.floor,
        leaves: &[],
        targets: &targets,
        stimuli: &stimuli,
        scripted: &scripted,
    };
    let mut out = AiOutcome::default();
    rig.world.tick(&ctx, &mut out);
    let rat_agent = rig.world.agent("rat").expect("rat");
    assert!(matches!(rat_agent.state, AiState::Scripted));
    assert_eq!(rat_agent.speed_mps, 0.0);
}

#[test]
fn removing_an_agent_releases_its_state() {
    let mut rig = Rig::new(
        Vec3::new(10.0, 0.0, 8.0),
        Vec3::new(5.0, 0.0, 8.0),
        prey_def(),
    );
    rig.tick(2);
    let prey = rig.prey;
    assert!(rig.world.remove(prey));
    assert!(rig.world.agent("rat").is_none());
    // The predator's target snapshot no longer resolves: it drops pursuit.
    rig.tick(2);
    let predator = rig.world.agent("cat").expect("cat");
    assert!(
        matches!(predator.state, AiState::Idle | AiState::Investigate { .. }),
        "a lost target releases the pursuit, got {:?}",
        predator.state
    );
}

/// Two rooms joined by one door; the door is the only link.
fn door_fixture() -> (LevelDef, CollisionWorld, Doors) {
    let level = LevelDef::from_json(
        r#"{
            "format_version": 3, "id": "ai_door_fixture", "name": "AI Door",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 3.0 },
                { "x": 8.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 3.0 }
            ],
            "walls": [ { "x": 7.85, "z": 0.0, "width": 0.3, "depth": 6.0, "height": 3.0,
                         "openings": [ { "kind": "door", "offset": 2.3, "width": 1.4,
                                         "height": 2.1, "sill": 0.0 } ] } ],
            "doors": [ { "id": "divider", "x": 8.0, "z": 2.3, "rotation_degrees": 270.0,
                         "width": 1.4, "height": 2.1, "initial_state": "closed" } ]
        }"#,
    )
    .expect("the door fixture parses");
    let collision = CollisionWorld::from_level(&level);
    let doors = Doors::from_level(&level);
    (level, collision, doors)
}

#[test]
fn a_door_capable_agent_requests_and_crosses_a_closed_door() {
    let (level, collision, mut doors) = door_fixture();
    let mesh = bake_mesh(&level, &collision, &doors);
    let mut store = EntityStore::new();
    let cat = store.insert();
    let rat = store.insert();
    let mut world = AiWorld::new();
    // The cat starts behind the closed door; the rat is a heard stimulus on
    // the far side, so the cat must open the door to reach it.
    world.register(
        cat,
        "cat",
        predator_def(),
        cat_body(true),
        Vec3::new(2.0, 0.0, 3.5),
        0.0,
        false,
    );
    let index = CollisionIndex::build(&collision.walls);
    let mut crossed = false;
    let mut requested = false;
    for tick in 0..(60 * 20) {
        let targets = world
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect::<Vec<_>>();
        let stimuli = vec![Stimulus {
            position: Vec3::new(13.0, 0.0, 3.7),
            radius: 12.0,
            loudness: 1.0,
            category: "movement".to_string(),
            source: Some(rat),
            age: 0.1,
        }];
        let leaves = doors.colliders();
        let ctx = AiTickContext {
            delta: 1.0 / 60.0,
            sim_time: 0.0,
            nav: Some(&mesh),
            doors: &doors,
            door_version: doors.version(),
            walls: &collision.walls,
            index: &index,
            floor: &collision.floor,
            leaves: &leaves,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &[],
        };
        let mut out = AiOutcome::default();
        world.tick(&ctx, &mut out);
        if out.door_requests.iter().any(|(door, _)| door == "divider") {
            requested = true;
            // The world applies a request through the ordinary door state
            // machine; the test plays that part.
            doors.request_open("divider");
        }
        // Advance any moving leaf so it can settle open.
        for _ in 0..4 {
            doors.advance(1.0 / 60.0, |_, _| false);
        }
        if world
            .agent("cat")
            .is_some_and(|agent| agent.position.x > 8.5)
        {
            crossed = true;
            break;
        }
        let _ = tick;
    }
    assert!(requested, "the cat asks for the door on its route");
    assert!(crossed, "the cat crosses once the door is open");
}

#[test]
fn a_doorless_prey_never_requests_a_closed_door() {
    let (level, collision, mut doors) = door_fixture();
    let mesh = bake_mesh(&level, &collision, &doors);
    let mut store = EntityStore::new();
    let rat = store.insert();
    let mut world = AiWorld::new();
    world.register(
        rat,
        "rat",
        prey_def(),
        rat_body(false),
        Vec3::new(2.0, 0.0, 3.5),
        0.0,
        false,
    );
    let index = CollisionIndex::build(&collision.walls);
    let mut requested = false;
    for _ in 0..(60 * 10) {
        let targets = world
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect::<Vec<_>>();
        let stimuli: Vec<Stimulus> = Vec::new();
        let leaves = doors.colliders();
        let ctx = AiTickContext {
            delta: 1.0 / 60.0,
            sim_time: 0.0,
            nav: Some(&mesh),
            doors: &doors,
            door_version: doors.version(),
            walls: &collision.walls,
            index: &index,
            floor: &collision.floor,
            leaves: &leaves,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &[],
        };
        let mut out = AiOutcome::default();
        world.tick(&ctx, &mut out);
        requested |= !out.door_requests.is_empty();
        doors.advance(1.0 / 60.0, |_, _| false);
    }
    assert!(
        !requested,
        "an agent without the door capability never asks for a closed door"
    );
    assert!(
        world
            .agent("rat")
            .is_some_and(|agent| agent.position.x < 7.0),
        "the rat stays on its own side"
    );
}

#[test]
fn an_unreachable_investigate_goal_times_out_without_query_spam() {
    // The only link between the two rooms is locked, so the sound the cat
    // hears can never be reached. It must not pin in `investigate` forever,
    // and it must not re-path every frame while the goal stays impossible.
    let (level, collision, mut doors) = door_fixture();
    doors.set_locked("divider", true);
    let mesh = bake_mesh(&level, &collision, &doors);
    let mut store = EntityStore::new();
    let cat = store.insert();
    let rat = store.insert();
    let mut world = AiWorld::new();
    world.register(
        cat,
        "cat",
        predator_def(),
        cat_body(true),
        Vec3::new(2.0, 0.0, 3.5),
        0.0,
        false,
    );
    let index = CollisionIndex::build(&collision.walls);
    let mut queries = 0usize;
    let mut entered = false;
    let mut recovered = false;
    for _ in 0..(60 * 8) {
        let targets = world
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect::<Vec<_>>();
        let stimuli = vec![Stimulus {
            position: Vec3::new(13.0, 0.0, 3.7),
            radius: 12.0,
            loudness: 1.0,
            category: "movement".to_string(),
            source: Some(rat),
            age: 0.1,
        }];
        let leaves = doors.colliders();
        let ctx = AiTickContext {
            delta: 1.0 / 60.0,
            sim_time: 0.0,
            nav: Some(&mesh),
            doors: &doors,
            door_version: doors.version(),
            walls: &collision.walls,
            index: &index,
            floor: &collision.floor,
            leaves: &leaves,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &[],
        };
        let mut out = AiOutcome::default();
        world.tick(&ctx, &mut out);
        queries = queries.saturating_add(out.path_queries);
        if matches!(
            world.agent("cat").expect("cat").state,
            AiState::Investigate { .. }
        ) {
            entered = true;
        } else if entered {
            recovered = true;
            break;
        }
        doors.advance(1.0 / 60.0, |_, _| false);
    }
    assert!(entered, "the cat investigates the sound it heard");
    assert!(
        recovered,
        "an unreachable goal must time out, got {:?}",
        world.agent("cat").map(|agent| agent.state)
    );
    assert!(
        queries <= 12,
        "retries are bounded, performed {queries} path queries"
    );
}

#[test]
fn a_wanderer_with_no_route_still_returns_to_its_post() {
    // A wanderer whose wander picks land in the other room (no door) must
    // still leave `wander` and keep its post.
    let (level, collision, doors) = door_fixture();
    let mesh = bake_mesh(&level, &collision, &doors);
    let mut store = EntityStore::new();
    let rat = store.insert();
    let mut world = AiWorld::new();
    let mut def = predator_def();
    def.behavior = AiBehavior::Wanderer;
    def.wander_radius = 8.0;
    world.register(
        rat,
        "rat",
        def,
        rat_body(false),
        Vec3::new(2.0, 0.0, 3.5),
        0.0,
        false,
    );
    let index = CollisionIndex::build(&collision.walls);
    let mut moved = 0.0_f32;
    let start = Vec3::new(2.0, 0.0, 3.5);
    let mut stuck_in_wander = true;
    for _ in 0..(60 * 12) {
        let targets = world
            .agents()
            .iter()
            .map(|agent| AiTarget {
                handle: agent.handle,
                instance_id: agent.instance_id.clone(),
                role: agent.role.clone(),
                behavior: agent.def.behavior,
                position: agent.position,
                radius: agent.profile.radius,
                height: agent.profile.height,
                caught: agent.caught,
            })
            .collect::<Vec<_>>();
        let stimuli: Vec<Stimulus> = Vec::new();
        let leaves = doors.colliders();
        let ctx = AiTickContext {
            delta: 1.0 / 60.0,
            sim_time: 0.0,
            nav: Some(&mesh),
            doors: &doors,
            door_version: doors.version(),
            walls: &collision.walls,
            index: &index,
            floor: &collision.floor,
            leaves: &leaves,
            targets: &targets,
            stimuli: &stimuli,
            scripted: &[],
        };
        let mut out = AiOutcome::default();
        world.tick(&ctx, &mut out);
        let agent = world.agent("rat").expect("rat");
        moved = moved.max(agent.position.distance(start));
        if !matches!(agent.state, AiState::Wander { .. }) {
            stuck_in_wander = false;
        }
    }
    assert!(
        !stuck_in_wander,
        "the wanderer must leave the wander state when its pick fails"
    );
    assert!(
        moved > 0.01,
        "the wanderer must still move within its own room, moved {moved}"
    );
}

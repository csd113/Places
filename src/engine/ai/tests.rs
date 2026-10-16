//! AI framework tests: perception, behavior transitions, hearing, catching
//! and the animation bridge. These exercise [`AiWorld`] directly with the real
//! baked navigation of a generated level.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic
// here; the production lints stay enforced everywhere else.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use super::*;
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::entities::id::EntityStore;
use crate::engine::level::LevelDef;
use crate::engine::nav::{
    NAV_MAX_SLOPE, NavMesh, NavScratch, PathQuery, PathResult, reference_class,
};
use crate::game::CollisionWorld;
use crate::map_compiler::navigation::{NavBakeInput, NavBakeOptions, bake};

/// The shelter regression needs only an 8x4 m room: one wall requires a
/// detour, and another seals the closest-looking unreachable shelter.
/// This protects player perception and the whole hide/return presentation in
/// one fixture, without loading a shipped map or baking lighting.
#[test]
fn skittish_player_retreat_uses_nearest_complete_route_and_returns_visibly() {
    let level = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"shelters","name":"Shelters",
        "spawn":{"x":3.2,"z":1.4},
        "rooms":[{"x":0,"z":0,"width":8,"depth":4,"height":3}],
        "walls":[
            {"x":3.9,"z":0,"width":0.2,"depth":3,"height":3},
            {"x":6,"z":0,"width":0.2,"depth":4,"height":3}
        ],
        "props":[{"id":"insect","model":"core:crate","x":3.2,"z":1,
            "solid":false,"components":[
                {"component":"nav_agent","radius":0.035,"height":0.035,
                 "speed_mps":1,"step_height":0.1,"max_slope":1},
                {"component":"ai","behavior":"skittish","reacts_to":["player"],
                 "walk_speed":1,"run_speed":2,"sight_range":4,"sight_fov_degrees":360,
                 "hearing_range":0,"flee_distance":3,"idle_seconds":0.5,
                 "hide_seconds":0.25,"wander_radius":1,
                 "hide_anchors":[[4.5,0,1],[1.5,0,1],[7,0,1]],
                 "home_bounds":{"x":0.2,"z":0.2,"width":7.6,"depth":3.6,"min_y":-0.05,"max_y":0.05}}
            ]}]
    }"#,
    )
    .expect("the tiny shelter fixture parses");
    let collision = CollisionWorld::from_level(&level);
    let index = CollisionIndex::build(&collision.walls);
    let mut warnings = Vec::new();
    let (bytes, _) = crate::map_compiler::bake_navigation(&level, 1, &mut warnings)
        .expect("only the tiny fixture navigation bakes");
    let mesh = NavMesh::from_record(
        crate::engine::package::navigation::read_navigation(&bytes)
            .expect("the navigation record decodes"),
    )
    .expect("the navigation validates");
    let mut world = crate::engine::entities::EntityWorld::from_level(&level);
    let start = world.ai().agent("insect").expect("insect exists").position;
    let player = Vec3::new(3.2, 0.0, 1.4);
    let ctx = crate::engine::entities::WorldContext {
        delta_seconds: 0.05,
        feet_from: player,
        feet: player,
        eye: Vec3::new(player.x, 1.6, player.z),
        body_height: 1.8,
        walls: &collision.walls,
        index: &index,
        floor: &collision.floor,
        nav: Some(&mesh),
    };
    let _first_tick = world.tick(&ctx);
    let insect = world.ai().agent("insect").expect("insect exists");
    let AiState::Hide { destination } = insect.state else {
        panic!(
            "the real player target triggers a shelter retreat: {:?}",
            insect.state
        );
    };
    assert!(
        destination.distance(Vec3::new(1.5, 0.0, 1.0)) < 0.15,
        "short reachable route beats an adjacent shelter across a wall and a sealed shelter"
    );
    assert!(
        insect.position.distance(start) < 0.2,
        "retreat starts by walking, never teleporting"
    );
    let class = insect
        .class
        .expect("the authored small body has a baked class");
    let mut scratch = NavScratch::new();
    let mut detour = Path::new();
    let doors = Doors::from_level(&level);
    let query = PathQuery {
        class,
        start,
        goal: Vec3::new(4.5, 0.0, 1.0),
        can_open_doors: false,
        max_expansions: 4096,
        doors: &doors,
    };
    mesh.path_into(&query, &mut scratch, &mut detour)
        .expect("the unbounded wall detour exists");
    assert!(detour.complete, "the wall can be walked around in the room");
    let bounds = NavBounds {
        min: Vec3::new(0.2, -0.05, 0.2),
        max: Vec3::new(7.8, 0.05, 2.5),
    };
    let bounded = mesh.path_into_bounded(&query, &mut scratch, &mut detour, Some(bounds));
    assert!(
        bounded.is_err() || !detour.complete,
        "a route must not leave home bounds to walk around the wall"
    );

    let mut saw_hidden = false;
    let mut saw_return = false;
    let mut finished = false;
    for _ in 0_i32..160_i32 {
        let _tick = world.tick(&ctx);
        let agent = world.ai().agent("insect").expect("insect exists");
        let frame = world
            .entity_frames()
            .iter()
            .find(|frame| frame.instance_id == "insect")
            .expect("the renderer receives the insect frame");
        match agent.state {
            AiState::Hidden => {
                saw_hidden = true;
                assert!(
                    agent.position.distance(destination) < 0.045,
                    "tiny insect must enter the shelter before hiding"
                );
                assert!(frame.opacity < 0.001, "arrival hides the render frame");
            }
            AiState::Return { destination: _ } => {
                saw_return = true;
                assert!(
                    frame.opacity > 0.99,
                    "the bounded hold ends visibly despite a nearby player"
                );
            }
            AiState::Idle if saw_return => {
                assert!(
                    agent.position.distance(start) < 0.05,
                    "the insect walks back to its pre-retreat position"
                );
                assert!(
                    agent.shelter_cooldown > 0.0,
                    "emergence has a bounded respite"
                );
                finished = true;
                break;
            }
            AiState::Idle
            | AiState::Wander { destination: _ }
            | AiState::Hide { destination: _ }
            | AiState::Follow { target: _ }
            | AiState::Flee {
                threat: _,
                destination: _,
            }
            | AiState::Investigate { point: _ }
            | AiState::Pursue { target: _ }
            | AiState::Catch { target: _ }
            | AiState::Scripted
            | AiState::Caught => {
                assert!(frame.opacity > 0.99, "travelling and waiting stay visible");
            }
        }
    }
    assert!(
        saw_hidden && saw_return && finished,
        "retreat, bounded hiding and reachable return all finish"
    );
}

/// A 0.3 m strip is walkable for the reference class but outside the allowed
/// feet-height band. This catches A* or string pulling crossing a ramp/floor
/// transition even when both destination endpoints remain on the home floor.
#[test]
fn home_floor_band_excludes_traversable_height_transitions() {
    let level = LevelDef::from_json(
        r#"{
        "format_version":3,"id":"home_floor","name":"Home Floor",
        "spawn":{"x":1,"z":1},
        "rooms":[{"x":0,"z":0,"width":8,"depth":2,"height":3}],
        "floor_regions":[{"x":3,"z":0,"width":1,"depth":2,"offset_y":0.3}]
    }"#,
    )
    .expect("the narrow height fixture parses");
    let collision = CollisionWorld::from_level(&level);
    let doors = Doors::from_level(&level);
    let mesh = bake_mesh(&level, &collision, &doors);
    let query = PathQuery {
        class: 0,
        start: Vec3::new(1.0, 0.0, 1.0),
        goal: Vec3::new(6.0, 0.0, 1.0),
        can_open_doors: false,
        max_expansions: 4096,
        doors: &doors,
    };
    let mut scratch = NavScratch::new();
    let mut path = Path::new();
    mesh.path_into(&query, &mut scratch, &mut path)
        .expect("ordinary walkers cross the strip");
    assert!(path.complete, "the strip is a supported traversable step");
    let bounds = NavBounds {
        min: Vec3::new(0.0, -0.05, 0.0),
        max: Vec3::new(8.0, 0.05, 2.0),
    };
    let result = mesh.path_into_bounded(&query, &mut scratch, &mut path, Some(bounds));
    assert!(
        result.is_err() || !path.complete,
        "floor-bound agents cannot cross the raised surface"
    );
}

/// No bake is needed: the regression protects movement-selected named poses
/// during a blocked route, and rejects unsafe authored shelter limits.
#[test]
fn named_ai_poses_follow_actual_motion_and_shelter_fields_are_bounded() {
    let mut world = AiWorld::new();
    let mut store = EntityStore::new();
    let def = AiDef {
        behavior: AiBehavior::Wanderer,
        idle_clip: Some("pose_stand".to_string()),
        move_clip: Some("run_arms_forward".to_string()),
        ..AiDef::default()
    };
    world.register(
        store.insert(),
        "actor",
        def.clone(),
        NavAgentProfile::reference(),
        Vec3::ZERO,
        0.0,
        false,
    );
    let actor = world.agents.first_mut().expect("actor exists");
    actor.state = AiState::Wander {
        destination: Vec3::X,
    };
    assert_eq!(
        actor.cue().clip_name(),
        Some("pose_stand"),
        "blocked movement holds the rest pose"
    );
    actor.speed_mps = 1.45;
    assert_eq!(
        actor.cue().clip_name(),
        Some("run_arms_forward"),
        "actual motion selects the authored clip"
    );
    let mut malformed = def;
    malformed.behavior = AiBehavior::Skittish;
    assert!(
        !malformed.is_valid(),
        "skittish behavior requires an authored shelter"
    );
    malformed.hide_anchors.push([1.0, 0.0, 1.0]);
    malformed.hide_seconds = 61.0;
    assert!(!malformed.is_valid(), "hidden holds remain bounded");
    malformed.hide_seconds = 0.25;
    assert!(
        malformed.is_valid(),
        "finite small shelter definitions remain usable"
    );
}

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
        crate::engine::package::navigation::NavClass::new(
            0.1,
            0.2,
            0.2,
            crate::engine::nav::NAV_MAX_SLOPE,
        )
        .expect("rat class"),
        crate::engine::package::navigation::NavClass::new(
            0.2,
            0.5,
            0.3,
            crate::engine::nav::NAV_MAX_SLOPE,
        )
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
        max_slope: crate::engine::nav::NAV_MAX_SLOPE,
        can_open_doors,
    }
}

fn cat_body(can_open_doors: bool) -> NavAgentProfile {
    NavAgentProfile {
        radius: 0.2,
        height: 0.5,
        step_height: 0.3,
        max_slope: crate::engine::nav::NAV_MAX_SLOPE,
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
    drop(rig.tick(1));
    let predator = rig.world.agent("cat").expect("the cat exists");
    assert!(
        matches!(predator.state, AiState::Pursue { target: _ }),
        "the cat pursues what it sees, got {:?}",
        predator.state
    );

    // The dividing wall (x = 11.7, z 0..6) blocks the same distance.
    let mut blocked = Rig::new(
        Vec3::new(14.0, 0.0, 3.0),
        Vec3::new(6.0, 0.0, 3.0),
        prey_def(),
    );
    drop(blocked.tick(1));
    let blocked_predator = blocked.world.agent("cat").expect("the cat exists");
    assert!(
        !matches!(blocked_predator.state, AiState::Pursue { target: _ }),
        "a wall blocks sight, got {:?}",
        blocked_predator.state
    );

    // Out of range: no pursuit either.
    let mut far = Rig::new(
        Vec3::new(23.0, 0.0, 10.0),
        Vec3::new(0.5, 0.0, 10.0),
        prey_def(),
    );
    drop(far.tick(1));
    let far_predator = far.world.agent("cat").expect("the cat exists");
    assert!(!matches!(far_predator.state, AiState::Pursue { target: _ }));
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
    for _ in 0_i32..240_i32 {
        drop(rig.tick(1));
        let rat = rig.world.agent("rat").expect("rat");
        if matches!(
            rat.state,
            AiState::Flee {
                threat: _,
                destination: _
            }
        ) {
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
    let later_outcome = rig.tick(120);
    assert!(later_outcome.catches.is_empty(), "a catch fires once");
    let caught_rat = rig.world.agent("rat").expect("rat");
    assert_eq!(caught_rat.position, position, "a caught rat does not move");
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
        matches!(
            rat_agent.state,
            AiState::Flee {
                threat: _,
                destination: _
            }
        ),
        "a heard threat starts a flee, got {:?}",
        rat_agent.state
    );
    // Silence and distance: the rat settles back to idle.
    drop(rig.tick(300));
    let later_rat = rig.world.agent("rat").expect("rat");
    assert!(
        !matches!(
            later_rat.state,
            AiState::Flee {
                threat: _,
                destination: _
            }
        ),
        "the rat settles after the noise stops, got {:?}",
        later_rat.state
    );
}

#[test]
fn the_animation_bridge_maps_states_to_gaits() {
    assert_eq!(
        movement::cue_for(AiState::Idle, 0.0),
        crate::engine::entity::PoseCue::Idle
    );
    assert_eq!(
        movement::cue_for(AiState::Caught, 2.0),
        crate::engine::entity::PoseCue::Idle
    );
    assert_eq!(
        movement::cue_for(
            AiState::Flee {
                threat: None,
                destination: Vec3::ZERO
            },
            1.4
        ),
        crate::engine::entity::PoseCue::Walk { speed_mps: 1.4 }
    );
    assert_eq!(
        movement::cue_for(
            AiState::Pursue {
                target: EntityStore::new().insert()
            },
            1.9
        ),
        crate::engine::entity::PoseCue::Walk { speed_mps: 1.9 }
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
    drop(rig.tick(2));
    let prey = rig.prey;
    assert!(rig.world.remove(prey));
    assert!(rig.world.agent("rat").is_none());
    // The predator's target snapshot no longer resolves: it drops pursuit.
    drop(rig.tick(2));
    let predator = rig.world.agent("cat").expect("cat");
    assert!(
        matches!(
            predator.state,
            AiState::Idle | AiState::Investigate { point: _ }
        ),
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
    for tick in 0_i32..(60_i32 * 20_i32) {
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
            let _request_open_status = doors.request_open("divider");
        }
        // Advance any moving leaf so it can settle open.
        for _ in 0_i32..4_i32 {
            let _advance_status = doors.advance(1.0 / 60.0, |_, _| false);
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
    for _ in 0_i32..(60_i32 * 10_i32) {
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
        let _advance_status = doors.advance(1.0 / 60.0, |_, _| false);
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
    let _locked_changed = doors.set_locked("divider", true);
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
    for _ in 0_i32..(60_i32 * 8_i32) {
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
            AiState::Investigate { point: _ }
        ) {
            entered = true;
        } else if entered {
            recovered = true;
            break;
        }
        let _advance_status = doors.advance(1.0 / 60.0, |_, _| false);
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
    for _ in 0_i32..(60_i32 * 12_i32) {
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
        if !matches!(agent.state, AiState::Wander { destination: _ }) {
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

// ---------------------------------------------------------------------------
// The shared mover on bounded step controls
// ---------------------------------------------------------------------------

/// A bounded control with its bake, collision world and live doors, for
/// driving the shared [`AgentMove`] through the authored geometry.
struct NavigationFixture {
    collision: CollisionWorld,
    doors: Doors,
    index: CollisionIndex,
    mesh: NavMesh,
}

impl NavigationFixture {
    fn new(source: &str) -> Self {
        let level = LevelDef::from_json(source).expect("the bounded navigation control parses");
        let mut warnings = Vec::new();
        let (bytes, _report) = crate::map_compiler::bake_navigation(&level, 1, &mut warnings)
            .expect("the bounded navigation control bakes");
        assert!(
            warnings.is_empty(),
            "the control has no navigation placement warnings: {warnings:?}"
        );
        let grid = crate::engine::package::navigation::read_navigation(&bytes)
            .expect("the record decodes");
        let mesh = NavMesh::from_record(grid).expect("the mesh validates");
        let collision = CollisionWorld::from_level(&level);
        let doors = Doors::from_level(&level);
        let index = CollisionIndex::build(&collision.walls);
        Self {
            collision,
            doors,
            index,
            mesh,
        }
    }

    /// The baked class index of one body profile.
    fn class(&self, profile: &NavAgentProfile) -> usize {
        self.mesh
            .class_index(&profile.class())
            .expect("the control bakes this body")
    }
}

/// What one traversal through the shared mover did.
struct MoverTraversal {
    /// Every feet height the mover visited, in order.
    ys: Vec<f32>,
    /// Frames simulated.
    frames: usize,
    /// The mover consumed the whole route.
    reached: bool,
}

impl MoverTraversal {
    /// True when the mover stood at `y` (within `tolerance`) at any point.
    fn saw_y(&self, y: f32, tolerance: f32) -> bool {
        self.ys.iter().any(|sample| (sample - y).abs() <= tolerance)
    }

    fn min_y(&self) -> f32 {
        self.ys.iter().copied().fold(f32::INFINITY, f32::min)
    }

    fn max_y(&self) -> f32 {
        self.ys.iter().copied().fold(f32::NEG_INFINITY, f32::max)
    }

    fn last_y(&self) -> f32 {
        self.ys.last().copied().unwrap_or(f32::NAN)
    }
}

/// Drives the shared [`AgentMove`] along a complete baked route exactly the
/// way the AI path follower does: target the current waypoint, advance on
/// arrival, fixed 60 Hz steps, no teleporting. Every frame asserts the body
/// stays on a walkable baked cell of its class and never falls through the
/// surface under it.
fn drive_shared_mover(
    fixture: &NavigationFixture,
    class: usize,
    start: Vec3,
    goal: Vec3,
    profile: &NavAgentProfile,
    speed_mps: f32,
    max_frames: usize,
) -> MoverTraversal {
    let mut scratch = NavScratch::new();
    let result = fixture.mesh.path(
        &PathQuery {
            class,
            start,
            goal,
            can_open_doors: false,
            max_expansions: 65536,
            doors: &crate::engine::nav::NoDoors,
        },
        &mut scratch,
    );
    let PathResult::Path(path) = result else {
        panic!("the demo traversal {start:?} -> {goal:?} must bake a route");
    };
    assert!(path.complete, "the traversal route must be complete");
    let mut mover = AgentMove {
        position: start,
        yaw_degrees: 0.0,
        radius: profile.radius,
        height: profile.height,
        step_height: profile.step_height,
        max_slope: profile.max_slope,
        speed_mps,
    };
    let leaves = fixture.doors.colliders();
    let mut current = 0usize;
    let mut ys = Vec::with_capacity(max_frames.min(4096));
    let mut frames = 0usize;
    let mut previous = start;
    for _ in 0..max_frames {
        let Some(waypoint) = path.waypoints.get(current).copied() else {
            break;
        };
        frames = frames.saturating_add(1);
        match mover.step_with_leaves(
            waypoint,
            1.0 / 60.0,
            &fixture.collision.walls,
            &fixture.index,
            &fixture.collision.floor,
            &leaves,
        ) {
            MoveStep::Moved {
                position: _,
                yaw_degrees: _,
                speed_mps: _,
            } => {
                let moved =
                    glam::Vec2::new(mover.position.x - previous.x, mover.position.z - previous.z)
                        .length();
                assert!(
                    moved <= speed_mps / 60.0 + 1.0e-3,
                    "the mover teleported {moved} m in one frame at {:?}",
                    mover.position
                );
                assert_on_baked_navigation(fixture, class, mover.position, profile.height);
                ys.push(mover.position.y);
                previous = mover.position;
                if waypoint.distance(mover.position) < movement::ARRIVE_RADIUS_M {
                    current = current.saturating_add(1);
                }
            }
            MoveStep::Arrived => {
                current = current.saturating_add(1);
            }
            MoveStep::Blocked => {
                panic!(
                    "the shared mover was blocked at {:?} targeting {waypoint:?}",
                    mover.position
                );
            }
        }
        if current >= path.waypoints.len() {
            break;
        }
    }
    MoverTraversal {
        ys,
        frames,
        reached: current >= path.waypoints.len(),
    }
}

/// Asserts the mover stays on its class's baked navigation: a walkable cell
/// of the class is within half a metre, and no static box blocks the body
/// where it stands. The mover may legally skim a cell corner the half-cell
/// clearance sampler stepped past, so the check is the nearest-cell contract
/// the encounter tests use, not an exact cell membership.
fn assert_on_baked_navigation(
    fixture: &NavigationFixture,
    class: usize,
    position: Vec3,
    height: f32,
) {
    let Some(point) = fixture.mesh.nearest(
        class,
        position,
        0.6,
        1.0,
        &crate::engine::nav::NoDoors,
        false,
    ) else {
        panic!("the mover left baked navigation at {position:?}");
    };
    assert!(
        point.position.distance(position) < 0.6,
        "the mover left baked navigation at {position:?} (nearest {:?})",
        point.position
    );
    for wall in &fixture.collision.walls {
        let inside = position.x > wall.min_x - 1.0e-3
            && position.x < wall.max_x + 1.0e-3
            && position.z > wall.min_z - 1.0e-3
            && position.z < wall.max_z + 1.0e-3
            && wall.blocks_body(position.y, height);
        assert!(
            !inside,
            "the mover entered {wall:?} at {position:?} in the demo traversal"
        );
    }
}

/// The shared mover walks the real Demo's two 0.3 m shower steps from the
/// pool deck into the bay and back: it climbs the heights, stays on baked
/// cells, never falls through and finishes in bounded time.
#[test]
fn the_shared_mover_climbs_the_control_shower_steps_and_returns() {
    let fixture = NavigationFixture::new(include_str!(
        "../../../tests/fixtures/unit/navigation_steps.json"
    ));
    let profile = NavAgentProfile {
        radius: 0.2,
        height: 0.45,
        step_height: 0.3,
        max_slope: NAV_MAX_SLOPE,
        can_open_doors: false,
    };
    let class = fixture.class(&profile);
    let deck = Vec3::new(23.4, -1.5, 10.0);
    let bay = Vec3::new(29.4, -0.9, 9.8);

    let out = drive_shared_mover(&fixture, class, deck, bay, &profile, 1.0, 60 * 60);
    assert!(out.reached, "the mover must reach the bay in bounded time");
    assert!(
        out.frames < 60 * 40,
        "the 7 m shower route takes well under 40 s, took {} frames",
        out.frames
    );
    assert!(
        (out.max_y() + 0.9).abs() < 0.05,
        "the mover tops out on the bay floor at -0.9, got {}",
        out.max_y()
    );
    assert!(
        out.saw_y(-1.2, 0.03),
        "the mover must stand on the first step (-1.2), ys {:?}",
        out.ys
    );
    assert!(out.saw_y(-0.9, 0.03), "the mover must stand in the bay");
    assert!(
        out.min_y() >= -1.5 - 1.0e-3,
        "the mover never falls through the deck, min y {}",
        out.min_y()
    );

    let back = drive_shared_mover(&fixture, class, bay, deck, &profile, 1.0, 60 * 60);
    assert!(back.reached, "the mover must return to the deck");
    assert!(back.frames < 60 * 40, "the return takes under 40 s");
    assert!(
        (back.last_y() + 1.5).abs() < 0.05,
        "the return ends on the deck at -1.5, got {}",
        back.last_y()
    );
    assert!(
        back.saw_y(-1.2, 0.03),
        "the return uses the first step (-1.2)"
    );
    assert!(
        back.max_y() <= -0.85,
        "the return never rises above the bay floor, got {}",
        back.max_y()
    );
    assert!(back.min_y() >= -1.5 - 1.0e-3);
}

/// The shared mover climbs the real Demo's 8 x 0.2625 m Home staircase and
/// actually displaces onto the balcony (1.2).
#[test]
fn the_shared_mover_climbs_the_control_home_staircase_to_the_balcony() {
    let fixture = NavigationFixture::new(include_str!(
        "../../../tests/fixtures/unit/navigation_stairs.json"
    ));
    let profile = NavAgentProfile {
        radius: 0.2,
        height: 0.45,
        step_height: 0.3,
        max_slope: NAV_MAX_SLOPE,
        can_open_doors: false,
    };
    let class = fixture.class(&profile);
    let lower = Vec3::new(54.5, -0.9, 12.0);
    let balcony = Vec3::new(60.0, 1.2, 12.5);

    let climb = drive_shared_mover(&fixture, class, lower, balcony, &profile, 1.0, 60 * 120);
    assert!(climb.reached, "the mover must finish the stair route");
    assert!(
        climb.frames < 60 * 90,
        "the stair route takes well under 90 s, took {} frames",
        climb.frames
    );
    assert!(
        (climb.last_y() - 1.2).abs() < 0.05,
        "the mover ends on the balcony at 1.2, got {}",
        climb.last_y()
    );
    assert!(
        climb.ys.iter().any(|y| (-0.6..0.0).contains(y)),
        "the mover climbs the lower flight, ys {:?}",
        climb.ys
    );
    assert!(
        climb.ys.iter().any(|y| (0.3..1.0).contains(y)),
        "the mover climbs the upper flight, ys {:?}",
        climb.ys
    );
    assert!(
        climb.min_y() >= -0.9 - 1.0e-3,
        "the mover never falls below the lower floor, min y {}",
        climb.min_y()
    );
}

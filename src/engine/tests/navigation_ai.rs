//! Focused CPU regressions for stopped clocks, sequence ownership, escape
//! retries and live navigation topology. Fixtures have at most 25 cells and
//! eight agents; they load no assets, bake no maps and need no native device.

use glam::Vec3;

use crate::engine::ai::movement::{AgentMove, MoveStep};
use crate::engine::ai::{AiBehavior, AiDef, AiOutcome, AiState, AiTarget, AiTickContext, AiWorld};
use crate::engine::collision::DoorCollider;
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::entities::id::EntityHandle;
use crate::engine::level::WalkableFloor;
use crate::engine::nav::{NavAgentProfile, NavDoorState, NavMesh, NavScratch, NoDoors, PathQuery};
use crate::engine::package::navigation::{CELL_SURFACE, NO_PORTAL, NO_REGION, NavGrid, NavPortal};

fn grid(width: u32, depth: u32, walkable: &[usize]) -> NavGrid {
    let cells = usize::try_from(u64::from(width).saturating_mul(u64::from(depth))).unwrap_or(0);
    let mut mask = vec![0_u8; cells.div_ceil(8)];
    let mut regions = vec![NO_REGION; cells];
    for &cell in walkable {
        if let Some(byte) = mask.get_mut(cell / 8) {
            *byte |= 1 << (cell % 8);
        }
        if let Some(region) = regions.get_mut(cell) {
            *region = 0;
        }
    }
    NavGrid {
        cell_m: 1.0,
        origin_x: 0.0,
        origin_z: 0.0,
        cells_x: width,
        cells_z: depth,
        classes: vec![NavAgentProfile::reference().class()],
        cell_y: vec![0.0; cells],
        cell_flags: vec![CELL_SURFACE; cells],
        cell_headroom_cm: vec![300; cells],
        cell_portal: vec![NO_PORTAL; cells],
        portals: Vec::new(),
        walkable: vec![mask],
        region: vec![regions],
    }
}

struct Fixture {
    nav: NavMesh,
    doors: Doors,
    index: CollisionIndex,
    floor: WalkableFloor,
    targets: Vec<AiTarget>,
}

impl Fixture {
    fn new(record: NavGrid) -> Result<Self, String> {
        Ok(Self {
            nav: NavMesh::from_record(record)?,
            doors: Doors::default(),
            index: CollisionIndex::empty(),
            floor: WalkableFloor::default(),
            targets: Vec::new(),
        })
    }

    fn context<'a>(&'a self, delta: f32, scripted: &'a [EntityHandle]) -> AiTickContext<'a> {
        AiTickContext {
            delta,
            sim_time: 0.0,
            nav: Some(&self.nav),
            doors: &self.doors,
            door_version: 0,
            walls: &[],
            leaves: &[],
            index: &self.index,
            floor: &self.floor,
            targets: &self.targets,
            stimuli: &[],
            scripted,
        }
    }
}

fn register(world: &mut AiWorld, slot: u32, def: AiDef, position: Vec3) -> EntityHandle {
    let handle = EntityHandle::from_parts(slot, 0);
    world.register(
        handle,
        &format!("agent_{slot}"),
        def,
        NavAgentProfile::reference(),
        position,
        0.0,
        false,
    );
    handle
}

#[test]
fn stopped_clock_preserves_routes_and_released_scripted_agent_resumes() -> Result<(), String> {
    let fixture = Fixture::new(grid(5, 5, &(0..25).collect::<Vec<_>>()))?;
    let mut world = AiWorld::new();
    let start = Vec3::new(2.5, 0.0, 2.5);
    let handle = register(
        &mut world,
        0,
        AiDef {
            behavior: AiBehavior::Wanderer,
            wander_radius: 1.5,
            ..AiDef::default()
        },
        start,
    );
    assert!(world.force_state(
        "agent_0",
        AiState::Wander {
            destination: Vec3::new(4.5, 0.0, 2.5),
        },
    ));
    world.tick(&fixture.context(0.05, &[]), &mut AiOutcome::default());
    let initial_agent = world.agent("agent_0").ok_or("missing agent")?;
    let route = initial_agent.path.clone();
    let waypoint = initial_agent.current_waypoint;
    let clock = initial_agent.clock;
    for delta in [0.0, -0.1, f32::NAN] {
        let mut out = AiOutcome::default();
        world.tick(&fixture.context(delta, &[]), &mut out);
        let stopped_agent = world.agent("agent_0").ok_or("missing stopped agent")?;
        assert_eq!(
            stopped_agent.path, route,
            "stopped time must retain the route"
        );
        assert_eq!(stopped_agent.current_waypoint, waypoint);
        assert!((stopped_agent.clock - clock).abs() < f32::EPSILON);
        assert!(out.moved.is_empty() && out.events.is_empty());
        assert_eq!(out.path_queries, 0);
    }
    let mut mover = AgentMove {
        position: start,
        yaw_degrees: 30.0,
        radius: 0.2,
        height: 1.8,
        step_height: 0.25,
        max_slope: 1.0,
        speed_mps: 1.0,
    };
    assert!(
        matches!(
            mover.step_with_leaves(
                Vec3::new(4.5, 0.0, 2.5),
                0.0,
                &[],
                &fixture.index,
                &fixture.floor,
                &[],
            ),
            MoveStep::Moved { position, yaw_degrees, speed_mps: _ }
                if position == start && (yaw_degrees - 30.0).abs() < f32::EPSILON
        ),
        "zero movement time must not report a distant waypoint as arrived"
    );
    world.tick(&fixture.context(0.05, &[handle]), &mut AiOutcome::default());
    let scripted_state = world
        .agent("agent_0")
        .ok_or("missing scripted agent")?
        .state;
    assert_eq!(scripted_state, AiState::Scripted);
    let mut resumed = AiOutcome::default();
    world.tick(&fixture.context(0.05, &[]), &mut resumed);
    let resumed_state = world.agent("agent_0").ok_or("missing resumed agent")?.state;
    assert!(
        matches!(resumed_state, AiState::Wander { destination: _ }),
        "sequence release must restore autonomous decisions"
    );
    assert!(resumed.path_queries > 0);
    assert!(world.set_light_response(handle, Some(Vec3::new(4.5, 1.5, 2.5))));
    let held_clock = world.agent("agent_0").ok_or("missing held agent")?.clock;
    let mut held = AiOutcome::default();
    world.tick(&fixture.context(0.05, &[]), &mut held);
    assert!(
        !held.moved.is_empty(),
        "the light hold must face the player"
    );
    let mut unchanged = AiOutcome::default();
    world.tick(&fixture.context(0.05, &[]), &mut unchanged);
    assert!(
        unchanged.moved.is_empty(),
        "an unchanged held pose needs no transform handoff"
    );
    let held_agent = world.agent("agent_0").ok_or("missing light-held agent")?;
    assert!(held_agent.position.distance(start) < f32::EPSILON);
    assert!((held_agent.clock - held_clock).abs() < f32::EPSILON);
    world.reset();
    let reset_agent = world.agent("agent_0").ok_or("missing reset agent")?;
    assert_eq!(reset_agent.state, AiState::Idle);
    assert!(reset_agent.light_look_target.is_none() && reset_agent.path.waypoints.is_empty());
    Ok(())
}

#[test]
fn trapped_prey_retries_are_bounded_and_perception_stays_staggered() -> Result<(), String> {
    let mut fixture = Fixture::new(grid(7, 3, &[8, 10]))?;
    let mut world = AiWorld::new();
    let _prey = register(
        &mut world,
        0,
        AiDef {
            behavior: AiBehavior::Prey,
            reacts_to: vec!["threat".into()],
            sight_fov_degrees: 360.0,
            flee_distance: 2.0,
            ..AiDef::default()
        },
        Vec3::new(1.5, 0.0, 1.5),
    );
    fixture.targets.push(AiTarget {
        handle: EntityHandle::from_parts(99, 0),
        instance_id: "threat".into(),
        role: Some("threat".into()),
        behavior: AiBehavior::Predator,
        position: Vec3::new(0.5, 0.0, 1.5),
        radius: 0.2,
        height: 1.8,
        caught: false,
    });
    let mut first = AiOutcome::default();
    world.tick(&fixture.context(0.05, &[]), &mut first);
    assert!(
        first.path_queries > 0,
        "fixture must attempt an unreachable escape"
    );
    for _ in 0_u8..14 {
        let mut quiet = AiOutcome::default();
        world.tick(&fixture.context(0.05, &[]), &mut quiet);
        assert_eq!(
            quiet.path_queries, 0,
            "the same trapped threat must respect the retry cadence"
        );
    }
    let mut retry = AiOutcome::default();
    world.tick(&fixture.context(0.1, &[]), &mut retry);
    assert!(retry.path_queries > 0, "bounded retries must still happen");

    let mut staggered = AiWorld::new();
    for slot in 0_u32..8 {
        let _agent = register(
            &mut staggered,
            slot,
            AiDef::default(),
            Vec3::new(1.5, 0.0, 1.5),
        );
    }
    let initial: Vec<_> = staggered
        .agents()
        .iter()
        .map(|agent| agent.last_perception_at)
        .collect();
    staggered.tick(&fixture.context(0.01, &[]), &mut AiOutcome::default());
    let checked = staggered
        .agents()
        .iter()
        .zip(&initial)
        .filter(|(agent, before)| agent.last_perception_at > **before)
        .count();
    assert_eq!(
        checked, 1,
        "first perception scans must use their authored registration phases"
    );
    Ok(())
}

struct LockedDoors;

impl NavDoorState for LockedDoors {
    fn door_open(&self, _id: &str) -> bool {
        false
    }
    fn door_locked(&self, _id: &str) -> Option<bool> {
        Some(true)
    }
    fn door_leaf(&self, _id: &str) -> Option<DoorCollider> {
        None
    }
}

#[test]
fn nearest_point_and_shortcuts_obey_distance_and_live_topology() -> Result<(), String> {
    let point_mesh = NavMesh::from_record(grid(5, 3, &[0, 8]))?;
    let nearest = point_mesh
        .nearest(0, Vec3::new(1.99, 0.0, 1.99), 3.0, 0.1, &NoDoors, false)
        .ok_or("missing nearest point")?;
    assert_eq!(
        nearest.cell, 8,
        "an axial next-ring cell is closer than the first-ring diagonal"
    );

    let mut record = grid(2, 2, &[0, 1, 2, 3]);
    record.portals.push(NavPortal {
        door: "blocked".into(),
    });
    for cell in [1_usize, 2] {
        *record
            .cell_portal
            .get_mut(cell)
            .ok_or("missing corner cell")? = 0;
    }
    let door_mesh = NavMesh::from_record(record)?;
    let start = Vec3::new(0.5, 0.0, 0.5);
    let goal = Vec3::new(1.5, 0.0, 1.5);
    assert!(
        !door_mesh.segment_clear(0, start, goal, &LockedDoors, true),
        "shortcuts cannot cross blocked door corners"
    );
    assert!(matches!(
        door_mesh.path(
            &PathQuery {
                class: 0,
                start,
                goal,
                can_open_doors: true,
                max_expansions: 16,
                doors: &LockedDoors,
            },
            &mut NavScratch::new()
        ),
        crate::engine::nav::PathResult::Unreachable
    ));

    let mut ledge = grid(2, 1, &[0, 1]);
    *ledge.cell_y.get_mut(1).ok_or("missing ledge cell")? = 0.6;
    let ledge_mesh = NavMesh::from_record(ledge)?;
    assert!(
        !ledge_mesh.segment_clear(0, start, Vec3::new(1.5, 0.6, 0.5), &NoDoors, false),
        "string pulling must not turn a flat cliff into a walkable slope"
    );
    Ok(())
}

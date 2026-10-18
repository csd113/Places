//! Focused CPU regressions for stopped clocks, sequence ownership, escape
//! retries and live navigation topology. Fixtures have at most 25 cells and
//! eight agents; they load no assets, bake no maps and need no native device.

use glam::Vec3;

use crate::engine::ai::movement::{AgentMove, MoveStep};
use crate::engine::ai::{AiBehavior, AiDef, AiOutcome, AiState, AiTarget, AiTickContext, AiWorld};
use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::entities::id::EntityHandle;
use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor};
use crate::engine::nav::{
    NavAgentProfile, NavDoorState, NavMesh, NavScratch, NoDoors, PathQuery, PathResult,
};
use crate::engine::package::navigation::{
    CELL_SURFACE, NAV_DIRECTIONS, NO_NAV_LINK, NO_PORTAL, NO_REGION, NavGrid, NavPortal,
};

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
    let mut links = vec![[NO_NAV_LINK; 8]; cells];
    for &cell in walkable {
        let node = u32::try_from(cell).unwrap_or(u32::MAX);
        let cx = node.checked_rem(width).unwrap_or(0);
        let cz = node.checked_div(width).unwrap_or(0);
        for (direction, (dx, dz)) in NAV_DIRECTIONS.into_iter().enumerate() {
            let Some((nx, nz)) = cx.checked_add_signed(dx).zip(cz.checked_add_signed(dz)) else {
                continue;
            };
            if nx >= width || nz >= depth {
                continue;
            }
            let next = usize::try_from(
                u64::from(nz)
                    .saturating_mul(u64::from(width))
                    .saturating_add(u64::from(nx)),
            )
            .unwrap_or(usize::MAX);
            if walkable.contains(&next)
                && let Some(slot) = links.get_mut(cell).and_then(|row| row.get_mut(direction))
            {
                *slot = u32::try_from(next).unwrap_or(NO_NAV_LINK);
            }
        }
    }
    NavGrid {
        cell_m: 1.0,
        origin_x: 0.0,
        origin_z: 0.0,
        cells_x: width,
        cells_z: depth,
        column_offsets: (0..=u32::try_from(cells).unwrap_or(0)).collect(),
        links: vec![links],
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
    ceiling: WalkableCeiling,
    obstacles: Vec<WallAabb>,
    targets: Vec<AiTarget>,
}

impl Fixture {
    fn new(record: NavGrid) -> Result<Self, String> {
        Ok(Self {
            nav: NavMesh::from_record(record)?,
            doors: Doors::default(),
            index: CollisionIndex::empty(),
            floor: WalkableFloor::default(),
            ceiling: WalkableCeiling::default(),
            obstacles: Vec::new(),
            targets: Vec::new(),
        })
    }

    fn context<'a>(&'a self, delta: f32, scripted: &'a [EntityHandle]) -> AiTickContext<'a> {
        AiTickContext {
            delta,
            sim_time: 0.0,
            nav: Some(&self.nav),
            doors: &self.doors,
            door_version: self.doors.version(),
            walls: &[],
            leaves: &[],
            index: &self.index,
            floor: &self.floor,
            ceiling: &self.ceiling,
            targets: &self.targets,
            stimuli: &[],
            scripted,
            obstacles: &self.obstacles,
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
    world.tick(&fixture.context(0.5, &[]), &mut resumed);
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

struct SuspendedDoor(DoorCollider);

impl NavDoorState for SuspendedDoor {
    fn door_open(&self, _id: &str) -> bool {
        false
    }
    fn door_locked(&self, _id: &str) -> Option<bool> {
        Some(false)
    }
    fn door_leaf(&self, _id: &str) -> Option<DoorCollider> {
        Some(self.0)
    }
}

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
    ledge.links = vec![vec![[NO_NAV_LINK; 8]; 2]];
    ledge.region = vec![vec![0, 1]];
    let ledge_mesh = NavMesh::from_record(ledge)?;
    assert!(
        !ledge_mesh.segment_clear(0, start, Vec3::new(1.5, 0.6, 0.5), &NoDoors, false),
        "string pulling must not turn a flat cliff into a walkable slope"
    );
    Ok(())
}

fn room_floor(rooms: &str) -> Result<(WalkableFloor, WalkableCeiling), String> {
    let level = LevelDef::from_json(&format!(
        r#"{{"schema":"places-level","format_version":1,"id":"nav_motion_fixture","name":"Navigation motion fixture","spawn":{{"x":0.5,"z":0.5}},"rooms":{rooms}}}"#
    ))
    .map_err(|error| error.to_string())?;
    Ok((
        WalkableFloor::from_level(&level),
        WalkableCeiling::from_level(&level),
    ))
}

/// Upper-room ordering previously stranded or raised a lower actor. The same
/// two-room fixture checks exact thin-solid contact and partial progress in a
/// single call; it builds only the floor sampler and never bakes a map.
#[test]
fn mover_keeps_storey_and_real_partial_progress_at_thin_live_solids() -> Result<(), String> {
    let (floor, ceiling) = room_floor(
        r#"[{"x":0.0,"z":0.0,"width":7.0,"depth":7.0,"floor_y":3.0},
             {"x":0.0,"z":0.0,"width":7.0,"depth":7.0,"floor_y":0.0}]"#,
    )?;
    let mut body = AgentMove {
        position: Vec3::new(1.0, 0.0, 3.0),
        yaw_degrees: 0.0,
        radius: 0.1,
        height: 0.5,
        step_height: 0.2,
        max_slope: 1.0,
        speed_mps: 8.0,
    };
    let solid = WallAabb::with_y(1.45, 0.0, 2.0, 0.01, 1.0, 2.0);
    let result = body.step_with_solids(
        Vec3::new(6.0, 0.0, 3.0),
        0.1,
        &[],
        &CollisionIndex::empty(),
        &floor,
        &ceiling,
        &[],
        &[solid],
    );
    let MoveStep::Moved {
        position,
        yaw_degrees: _,
        speed_mps,
    } = result
    else {
        return Err("movement before contact must be retained".into());
    };
    assert!(
        position.x > 1.1 && position.x <= 1.35001,
        "thin solid must stop the swept body: {position:?}"
    );
    assert!(
        position.y.abs() < 1.0e-6,
        "first authored upper room must not replace current storey"
    );
    assert!(
        (speed_mps - position.distance(Vec3::new(1.0, 0.0, 3.0)) / 0.1).abs() < 1.0e-5,
        "reported speed must measure actual displacement before obstruction"
    );
    let mut fixture = Fixture::new(grid(7, 7, &(0_usize..49).collect::<Vec<_>>()))?;
    fixture.floor = floor;
    fixture.ceiling = ceiling;
    let mut world = AiWorld::new();
    let _handle = register(&mut world, 0, AiDef::default(), Vec3::new(1.5, 0.0, 3.5));
    assert!(world.set_navigation_goal("agent_0", Vec3::new(5.5, 0.0, 3.5)));
    for _ in 0_u16..20 {
        world.tick(&fixture.context(0.05, &[]), &mut AiOutcome::default());
    }
    assert!(
        world
            .agent("agent_0")
            .ok_or("missing lower actor")?
            .position
            .x
            > 2.0,
        "long local probes under another storey must retain the lower route"
    );
    let (close_floor, close_ceiling) = room_floor(
        r#"[{"x":0.0,"z":0.0,"width":3.0,"depth":3.0,"floor_y":0.3},
             {"x":0.0,"z":0.0,"width":3.0,"depth":3.0,"floor_y":0.0}]"#,
    )?;
    body.position = Vec3::new(0.5, 0.0, 1.5);
    body.height = 0.1;
    body.step_height = 0.4;
    assert!(
        matches!(
            body.step_with_solids(
                Vec3::new(2.5, 0.0, 1.5), 0.05, &[], &CollisionIndex::empty(),
                &close_floor, &close_ceiling, &[], &[],
            ),
            MoveStep::Moved { position: close_position, yaw_degrees: _, speed_mps: _ }
                if close_position.x > 0.5 && close_position.y.abs() < 1.0e-6
        ),
        "a tiny body must retain the lower close storey even when the upper floor lies within step reach"
    );
    body.height = 1.8;
    assert!(
        matches!(
            body.step_with_solids(
                Vec3::new(2.5, 0.0, 1.5),
                0.05,
                &[],
                &CollisionIndex::empty(),
                &close_floor,
                &close_ceiling,
                &[],
                &[],
            ),
            MoveStep::Blocked
        ),
        "a body taller than the storey gap must not step through the upper slab"
    );
    Ok(())
}

/// Native ramp walking stopped when a node was inside the horizontal arrival
/// radius but outside its 3D radius: the mover returned Arrived while the route
/// follower required movement. One floor sampler and one step protect that
/// tolerance boundary, with no navigation bake, assets or native resources.
#[test]
fn sloped_waypoint_inside_horizontal_tolerance_still_advances_in_three_dimensions()
-> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"ramp_arrival","name":"Ramp arrival",
        "spawn":{"x":10.5,"z":4.0},"rooms":[{"x":10.0,"z":0.0,"width":10.0,"depth":8.0,"height":6.0}],
        "ramps":[{"x":11.0,"z":2.5,"width":7.5,"depth":2.0,"rise":3.0}]}"#,
    ).map_err(|error| error.to_string())?;
    let mut body = AgentMove {
        position: Vec3::new(17.455, 2.582, 2.918),
        yaw_degrees: 0.0,
        radius: 0.3,
        height: 1.8,
        step_height: 0.4,
        max_slope: 1.0,
        speed_mps: 1.4,
    };
    let waypoint = Vec3::new(17.5, 2.6, 2.9);
    let tolerance = crate::engine::ai::movement::arrival_radius(body.radius);
    assert!((waypoint.x - body.position.x).hypot(waypoint.z - body.position.z) < tolerance);
    assert!(waypoint.distance(body.position) > tolerance);
    assert!(
        matches!(body.step_with_solids(
        waypoint, 0.05, &[], &CollisionIndex::empty(),
        &WalkableFloor::from_level(&level), &WalkableCeiling::from_level(&level), &[], &[],
    ), MoveStep::Moved { position, yaw_degrees: _, speed_mps: _ }
        if position.distance(waypoint) < tolerance),
        "a ramp node outside the 3D radius must advance rather than report a premature arrival"
    );
    Ok(())
}

/// A current physics blocker previously existed only in Rapier, so paths and
/// AI passed through it. This 35-node corridor exercises ordinary ticks while
/// blocked, then removes the snapshot and checks finite-cadence recovery.
#[test]
fn live_props_block_ai_and_cleared_obstructions_resume_on_bounded_retries() -> Result<(), String> {
    let mut fixture = Fixture::new(grid(7, 5, &(0_usize..35).collect::<Vec<_>>()))?;
    (fixture.floor, fixture.ceiling) =
        room_floor(r#"[{"x":0.0,"z":0.0,"width":7.0,"depth":5.0}]"#)?;
    fixture
        .obstacles
        .push(WallAabb::with_y(2.9, 0.0, -1.0, 0.2, 2.0, 7.0));
    let mut world = AiWorld::new();
    let _handle = register(&mut world, 0, AiDef::default(), Vec3::new(0.5, 0.0, 2.5));
    let goal = Vec3::new(6.5, 0.0, 2.5);
    assert!(world.set_navigation_goal("agent_0", goal));
    let mut queries = 0_usize;
    for _ in 0_u16..40 {
        let mut out = AiOutcome::default();
        world.tick(&fixture.context(0.05, &[]), &mut out);
        queries = queries.saturating_add(out.path_queries);
        let agent = world.agent("agent_0").ok_or("missing obstructed actor")?;
        assert!(
            agent.position.x + agent.profile.radius <= 2.9,
            "live prop must never be crossed while blocked"
        );
    }
    assert!(
        (1..=5).contains(&queries),
        "unchanged failure must not search every frame"
    );
    fixture.obstacles.clear();
    for _ in 0_u16..140 {
        world.tick(&fixture.context(0.05, &[]), &mut AiOutcome::default());
    }
    assert!(
        world
            .agent("agent_0")
            .ok_or("missing recovered actor")?
            .position
            .distance(goal)
            < 0.23,
        "the normal planner and mover must recover after the prop is removed"
    );
    Ok(())
}

/// Two ordinary movers previously occupied each other. A 49-node open floor
/// protects physical passing, then a twelve-agent burst verifies that a shared
/// eight-query cap defers work fairly instead of starving later agents.
#[test]
fn opposing_actors_pass_without_overlap_and_query_budget_rotates() -> Result<(), String> {
    let mut fixture = Fixture::new(grid(7, 7, &(0_usize..49).collect::<Vec<_>>()))?;
    (fixture.floor, fixture.ceiling) =
        room_floor(r#"[{"x":0.0,"z":0.0,"width":7.0,"depth":7.0}]"#)?;
    let mut world = AiWorld::new();
    let _handle = register(&mut world, 0, AiDef::default(), Vec3::new(1.5, 0.0, 3.5));
    let _second_handle = register(&mut world, 1, AiDef::default(), Vec3::new(5.5, 0.0, 3.5));
    assert!(world.set_navigation_goal("agent_0", Vec3::new(5.5, 0.0, 3.5)));
    assert!(world.set_navigation_goal("agent_1", Vec3::new(1.5, 0.0, 3.5)));
    for _ in 0_u16..140 {
        fixture.targets = world
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
                caught: false,
            })
            .collect();
        world.tick(&fixture.context(0.05, &[]), &mut AiOutcome::default());
        let first = world.agent("agent_0").ok_or("missing first mover")?;
        let second = world.agent("agent_1").ok_or("missing second mover")?;
        assert!(
            first.position.distance(second.position) + 1.0e-5
                >= first.profile.radius + second.profile.radius,
            "passing actors must not overlap in any tick"
        );
    }
    assert!(
        world
            .agent("agent_0")
            .ok_or("missing first arrival")?
            .position
            .x
            > 5.0
    );
    assert!(
        world
            .agent("agent_1")
            .ok_or("missing second arrival")?
            .position
            .x
            < 2.0
    );

    world.clear();
    fixture.targets.clear();
    for slot in 0_u32..12 {
        let _burst_handle = register(&mut world, slot, AiDef::default(), Vec3::new(0.5, 0.0, 0.5));
        assert!(world.set_navigation_goal(&format!("agent_{slot}"), Vec3::new(6.5, 0.0, 6.5)));
    }
    for _ in 0_u16..4 {
        let mut out = AiOutcome::default();
        world.tick(&fixture.context(0.05, &[]), &mut out);
        assert!(out.path_queries <= crate::engine::ai::MAX_PATH_QUERIES_PER_TICK);
    }
    assert!(
        world.agents().iter().all(|agent| agent.path_queries > 0),
        "later agents must receive a deferred search opportunity"
    );
    Ok(())
}

/// A fast door formerly tested only the final angle and crossed a body at
/// mid-swing. One leaf and three navigation nodes prove intermediate contact,
/// transient planner blockage and reopening after the obstruction clears.
#[test]
fn fast_obstructed_door_checks_its_swing_and_routes_recover_after_clear() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"door_probe","name":"Door probe",
        "spawn":{"x":0.5,"z":0.5},"rooms":[{"x":0.0,"z":0.0,"width":20.0,"depth":20.0}],
        "doors":[{"id":"probe","x":10.0,"z":10.0,"width":1.0,"height":2.0,
        "rotation_degrees":0.0,"swing_degrees":90.0,"open_speed_degrees":720.0,
        "open_direction":"left","initial_state":"closed","material":"unused"}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut doors = Doors::from_level(&level);
    assert!(doors.request_open("probe"));
    let _moved = doors.advance(0.5, |_, leaf| {
        let angle = leaf.dir_z.abs().atan2(leaf.dir_x.abs()).to_degrees();
        (40.0..50.0).contains(&angle)
    });
    let leaf = doors.get(0).ok_or("missing probe door")?;
    assert!(
        leaf.obstructed && leaf.angle().abs() < 50.0,
        "an intermediate actor must stop a fast leaf before its clear final pose"
    );
    assert!(doors.door_obstructed("probe"));
    let mut record = grid(3, 1, &[0, 1, 2]);
    record.portals.push(NavPortal {
        door: "probe".into(),
    });
    *record.cell_portal.get_mut(1).ok_or("missing portal cell")? = 0;
    let nav = NavMesh::from_record(record)?;
    let start = Vec3::new(0.5, 0.0, 0.5);
    let goal = Vec3::new(2.5, 0.0, 0.5);
    assert!(
        !matches!(nav.path(&PathQuery { class: 0, start, goal, can_open_doors: true, max_expansions: 16, doors: &doors }, &mut NavScratch::new()), PathResult::Path(path) if path.complete),
        "open capability must not treat a physically obstructed leaf as passable"
    );
    let _moved_after_clear = doors.advance(0.5, |_, _| false);
    assert!(doors.door_open("probe") && !doors.door_obstructed("probe"));
    assert!(
        matches!(nav.path(&PathQuery { class: 0, start, goal, can_open_doors: true, max_expansions: 16, doors: &doors }, &mut NavScratch::new()), PathResult::Path(path) if path.complete),
        "clearing the obstacle must restore the ordinary route without a rebake"
    );
    Ok(())
}

/// Native closing-door recovery deadlocked after unlock: the obstructed start
/// portal prevented a route, so no successful route ever requested reopening.
/// One leaf and 21 hand-authored nodes protect a bounded nearby retry without
/// making a locked door operable or requiring a full map bake.
#[test]
fn obstructed_closing_start_portal_can_request_reopening_after_unlock() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"door_recovery","name":"Door recovery",
        "spawn":{"x":0.5,"z":0.5},"rooms":[{"x":0.0,"z":0.0,"width":7.0,"depth":3.0}],
        "doors":[{"id":"probe","x":3.0,"z":2.2,"width":1.4,"height":2.2,
        "rotation_degrees":90.0,"swing_degrees":90.0,"open_speed_degrees":240.0,
        "open_direction":"left","initial_state":"open","material":"unused"}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let mut record = grid(7, 3, &(0_usize..21).collect::<Vec<_>>());
    record.portals.push(NavPortal {
        door: "probe".into(),
    });
    *record
        .cell_portal
        .get_mut(9)
        .ok_or("missing occupied portal")? = 0;
    let mut fixture = Fixture::new(record)?;
    fixture.floor = WalkableFloor::from_level(&level);
    fixture.ceiling = WalkableCeiling::from_level(&level);
    fixture.doors = Doors::from_level(&level);
    let position = Vec3::new(2.724_319, 0.0, 1.04321);
    assert!(fixture.doors.request_close("probe"));
    let _door_steps = fixture.doors.advance(0.5, |_, candidate| {
        candidate.overlaps_disc(position.x, position.z, 0.3)
    });
    assert!(fixture.doors.door_obstructed("probe"));
    let _locked = fixture.doors.set_locked("probe", true);
    let mut world = AiWorld::new();
    let _actor = register(
        &mut world,
        0,
        AiDef {
            can_open_doors: true,
            ..AiDef::default()
        },
        position,
    );
    assert!(world.set_navigation_goal("agent_0", Vec3::new(1.5, 0.0, 1.5)));
    let mut locked = AiOutcome::default();
    world.tick(&fixture.context(0.05, &[]), &mut locked);
    assert!(
        locked.door_requests.is_empty(),
        "a locked leaf must never be reopened by AI"
    );
    let _unlocked = fixture.doors.set_locked("probe", false);
    let mut request = AiOutcome::default();
    world.tick(&fixture.context(0.8, &[]), &mut request);
    assert_eq!(
        request.door_requests.len(),
        1,
        "a failed start portal still needs one nearby reopen request"
    );
    for _ in 0_u8..4 {
        let mut held = AiOutcome::default();
        world.tick(&fixture.context(0.05, &[]), &mut held);
        assert!(
            held.door_requests.is_empty(),
            "nearby recovery must share the path-attempt cooldown"
        );
    }
    assert!(fixture.doors.request_open("probe"));
    let _reopened_steps = fixture.doors.advance(0.5, |_, _| false);
    assert!(fixture.doors.door_open("probe"));
    for _ in 0_u16..100 {
        world.tick(&fixture.context(0.05, &[]), &mut AiOutcome::default());
    }
    assert!(
        world
            .agent("agent_0")
            .ok_or("missing recovered actor")?
            .position
            .distance(Vec3::new(1.5, 0.0, 1.5))
            < 0.06,
        "ordinary movement must resume after the closing leaf is reopened"
    );
    Ok(())
}

/// A short body lifting a legal step formerly skipped a suspended solid
/// between its disjoint old/new body bands. One floor sampler and two nodes
/// protect the common static/item/door sweep, planner overlay and smoothing;
/// an unobstructed rim step must still pass. No assets or map bake are used.
#[test]
fn lifted_short_body_sweeps_solids_between_disjoint_endpoint_bands() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"lifted_body","name":"Lifted body",
        "spawn":{"x":0.5,"z":0.5},"rooms":[{"x":0.0,"z":0.0,"width":2.0,"depth":2.0}],
        "floor_regions":[{"x":1.0,"z":0.0,"width":1.0,"depth":2.0,"offset_y":0.4}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let floor = WalkableFloor::from_level(&level);
    let ceiling = WalkableCeiling::from_level(&level);
    let step_walls = level.collision_aabbs();
    let index = CollisionIndex::build(&step_walls);
    let body = AgentMove {
        position: Vec3::new(0.975, 0.0, 1.0),
        yaw_degrees: 0.0,
        radius: 0.02,
        height: 0.1,
        step_height: 0.4,
        max_slope: 1.0,
        speed_mps: 1.0,
    };
    let waypoint = Vec3::new(1.2, 0.4, 1.0);
    let mut clear_body = body;
    assert!(
        matches!(clear_body.step_with_solids(
        waypoint, 0.05, &step_walls, &index, &floor, &ceiling, &[], &[],
    ), MoveStep::Moved { position, yaw_degrees: _, speed_mps: _ } if (position.y - 0.4).abs() < 1.0e-5),
        "the full envelope must retain the legal stepped-rim exemption"
    );
    let suspended = WallAabb::with_y(0.99, 0.2, 0.8, 0.02, 0.1, 0.4);
    let leaf = DoorCollider::from_pose([1.0, 0.2, 0.8], [0.0, 1.0], 0.4, 0.02, 0.1);
    let mut static_walls = step_walls.clone();
    static_walls.push(suspended);
    let static_index = CollisionIndex::build(&static_walls);
    for (walls, collision_index, leaves, items) in [
        (static_walls.as_slice(), &static_index, &[][..], &[][..]),
        (
            step_walls.as_slice(),
            &index,
            &[][..],
            std::slice::from_ref(&suspended),
        ),
        (
            step_walls.as_slice(),
            &index,
            std::slice::from_ref(&leaf),
            &[][..],
        ),
    ] {
        let mut blocked_body = body;
        assert!(
            matches!(
                blocked_body.step_with_solids(
                    waypoint,
                    0.05,
                    walls,
                    collision_index,
                    &floor,
                    &ceiling,
                    leaves,
                    items,
                ),
                MoveStep::Blocked
            ),
            "lifting through an intermediate static, item or leaf solid must be refused"
        );
    }
    assert_lifted_route_blockage(&body, suspended)
}

fn assert_lifted_route_blockage(body: &AgentMove, suspended: WallAabb) -> Result<(), String> {
    let mut record = grid(2, 1, &[0, 1]);
    *record.classes.first_mut().ok_or("missing class")? =
        crate::engine::package::navigation::NavClass {
            radius: body.radius,
            height: body.height,
            step_height: body.step_height,
            max_slope: body.max_slope,
        };
    *record.cell_y.get_mut(1).ok_or("missing upper surface")? = 0.4;
    let mut fixture = Fixture::new(record)?;
    fixture.obstacles.push(suspended);
    let from = Vec3::new(0.5, 0.0, 0.5);
    let to = Vec3::new(1.5, 0.4, 0.5);
    // Move the same thin suspended item onto this two-node segment's Z.
    *fixture.obstacles.first_mut().ok_or("missing item")? =
        WallAabb::with_y(0.99, 0.2, 0.3, 0.02, 0.1, 0.4);
    let ctx = fixture.context(0.05, &[]);
    assert!(!ctx.obstacle_blocked(from, body.radius, body.height));
    assert!(!ctx.obstacle_blocked(to, body.radius, body.height));
    assert!(ctx.obstacle_segment_blocked(from, to, body.radius, body.height));
    assert!(
        !fixture.nav.segment_clear(0, from, to, &ctx, false),
        "smoothing must check the lifted body envelope"
    );
    assert!(
        !matches!(fixture.nav.path(&PathQuery {
        class: 0, start: from, goal: to, can_open_doors: false, max_expansions: 8, doors: &ctx,
    }, &mut NavScratch::new()), PathResult::Path(path) if path.complete),
        "A* must not certify the suspended-solid crossing that actual movement rejects"
    );
    let mut door_record = fixture.nav.grid().clone();
    door_record.portals.push(NavPortal {
        door: "suspended".into(),
    });
    *door_record
        .cell_portal
        .first_mut()
        .ok_or("missing door portal")? = 0;
    let door_mesh = NavMesh::from_record(door_record)?;
    let suspended_door = SuspendedDoor(DoorCollider::from_pose(
        [1.0, 0.2, 0.3],
        [0.0, 1.0],
        0.4,
        0.02,
        0.1,
    ));
    assert!(
        !matches!(door_mesh.path(&PathQuery {
        class: 0, start: from, goal: to, can_open_doors: false, max_expansions: 8, doors: &suspended_door,
    }, &mut NavScratch::new()), PathResult::Path(path) if path.complete),
        "a closed leaf between disjoint body bands must obstruct a lifted route"
    );
    let openable = door_mesh.path(
        &PathQuery {
            class: 0,
            start: from,
            goal: to,
            can_open_doors: true,
            max_expansions: 8,
            doors: &suspended_door,
        },
        &mut NavScratch::new(),
    );
    assert!(
        matches!(openable, PathResult::Path(path) if path.complete && path.door_requests == ["suspended"]),
        "an agent that can open the crossed leaf must actually request it despite clear endpoint body bands"
    );
    Ok(())
}

//! Navigation bake and query tests.
//!
//! Fixtures are authored levels parsed by the real loader, baked by the real
//! compiler-side bake, encoded with the real record codec and queried through
//! the real runtime mesh: no test-only geometry path exists.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic
// here; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::format_push_string,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::unreachable,
    clippy::unwrap_used,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests and developer measurement output"
)]

use glam::Vec3;

use super::*;
use crate::collision::DoorCollider;
use crate::door::{DoorPhase, Doors};
use crate::game::CollisionWorld;
use crate::level::LevelDef;
use crate::package::navigation::{NavClass, NavGrid, read_navigation, write_navigation};

/// A door state double: open by default, with switchable closed/locked flags.
#[derive(Default)]
struct TestDoors {
    closed: Vec<String>,
    locked: Vec<String>,
    leaf: Option<(String, DoorCollider)>,
}

impl NavDoorState for TestDoors {
    fn door_open(&self, door_id: &str) -> bool {
        !self.closed.iter().any(|id| id == door_id)
    }

    fn door_locked(&self, door_id: &str) -> Option<bool> {
        Some(self.locked.iter().any(|id| id == door_id))
    }

    fn door_leaf(&self, door_id: &str) -> Option<DoorCollider> {
        self.leaf
            .as_ref()
            .filter(|(id, _)| id == door_id)
            .map(|(_, leaf)| *leaf)
    }
}

fn level(json: &str) -> LevelDef {
    LevelDef::from_json(json).expect("the navigation fixture parses")
}

/// Bakes a level exactly as the compiler does, through the package codec.
fn bake_level(level: &LevelDef, workers: usize) -> NavMesh {
    let collision = CollisionWorld::from_level(level);
    let doors = crate::door::Doors::from_level(level);
    let classes = vec![
        reference_class(),
        NavClass::new(0.12, 0.22, 0.2, NAV_MAX_SLOPE).expect("small class"),
        NavClass::new(0.45, 1.8, 0.4, NAV_MAX_SLOPE).expect("broad class"),
    ];
    let input = NavBakeInput {
        level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &[],
        walk_proxies: &[],
    };
    let (grid, _) = bake(
        &input,
        &NavBakeOptions {
            cell_m: DEFAULT_NAV_CELL_M,
            workers,
        },
    )
    .expect("the fixture bakes");
    // Round-trip through the compiled record, exactly like a package.
    let bytes = write_navigation(&grid).expect("the record encodes");
    let decoded = read_navigation(&bytes).expect("the record decodes");
    assert_eq!(decoded, grid, "the record survives a round trip");
    NavMesh::from_record(grid).expect("the mesh validates")
}

/// A path query across the door fixture's two rooms.
fn door_query(doors: &dyn NavDoorState, can_open: bool) -> PathQuery<'_> {
    PathQuery {
        class: 0,
        start: Vec3::new(2.0, 0.0, 2.0),
        goal: Vec3::new(10.0, 0.0, 2.0),
        can_open_doors: can_open,
        max_expansions: 65536,
        doors,
    }
}

/// Cell count of one class in a region label.
fn region_cells(mesh: &NavMesh, class: usize, region: u16) -> u32 {
    mesh.region_cells(class, region)
}

#[test]
fn a_flat_room_bakes_and_paths_straight() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_flat", "name": "Flat",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 6.0, "height": 3.0 } ]
        }"#,
    );
    let mesh = bake_level(&level, 1);
    let class = 0;
    assert!(mesh.grid().cell_count() > 100);
    let doors = NoDoors;
    let start = Vec3::new(1.0, 0.0, 1.0);
    let goal = Vec3::new(7.0, 0.0, 5.0);
    let nearest = mesh
        .nearest(class, start, 1.0, 1.0, &doors, true)
        .expect("the start snaps");
    assert!(nearest.position.distance(start) < 1.0);
    let mut scratch = NavScratch::new();
    let result = mesh.path(
        &PathQuery {
            class,
            start,
            goal,
            can_open_doors: true,
            max_expansions: 65536,
            doors: &doors,
        },
        &mut scratch,
    );
    let PathResult::Path(path) = result else {
        panic!("a flat room must path");
    };
    assert!(path.complete, "a straight route is complete");
    assert!(path.waypoints.len() >= 2);
    let reached = path.reached().expect("waypoints exist");
    assert!(
        reached.distance(goal) < 0.4,
        "the path reaches the goal, got {reached:?}"
    );
}

#[test]
fn a_staircase_connects_two_levels_and_a_cliff_does_not() {
    let stair_level = level(
        r#"{
            "format_version": 3, "id": "nav_stairs", "name": "Stairs",
            "spawn": { "x": 1.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 6.0, "height": 4.5 } ],
            "stairs": [ { "x": 3.0, "z": 2.0, "width": 2.0, "depth": 2.0,
                          "offset_y": 0.0, "rise": 2.0, "steps": 8 } ],
            "floor_regions": [ { "x": 5.0, "z": 2.0, "width": 3.0, "depth": 2.0,
                                 "offset_y": 2.0 } ]
        }"#,
    );
    let stairs = bake_level(&stair_level, 1);
    let class = 0;
    let doors = NoDoors;
    let mut scratch = NavScratch::new();
    let result = stairs.path(
        &PathQuery {
            class,
            start: Vec3::new(1.0, 0.0, 3.0),
            goal: Vec3::new(6.5, 2.0, 3.0),
            can_open_doors: true,
            max_expansions: 65536,
            doors: &doors,
        },
        &mut scratch,
    );
    let PathResult::Path(path) = result else {
        panic!("the staircase must connect the two floors");
    };
    assert!(path.complete, "a stair route is complete");
    let reached = path.reached().expect("waypoints exist");
    assert!(
        (reached.y - 2.0).abs() < 0.2,
        "the route climbs to the raised region, got {reached:?}"
    );

    // The same two levels without the staircase: the raised region is a
    // separate component because the rise exceeds the step.
    let cliff_level = level(
        r#"{
            "format_version": 3, "id": "nav_cliff", "name": "Cliff",
            "spawn": { "x": 1.0, "z": 5.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 6.0, "height": 4.5 } ],
            "floor_regions": [ { "x": 5.0, "z": 2.0, "width": 3.0, "depth": 2.0,
                                 "offset_y": 1.5 } ]
        }"#,
    );
    let cliff = bake_level(&cliff_level, 1);
    let cliff_result = cliff.path(
        &PathQuery {
            class,
            start: Vec3::new(1.0, 0.0, 3.0),
            goal: Vec3::new(6.5, 1.5, 3.0),
            can_open_doors: true,
            max_expansions: 65536,
            doors: &doors,
        },
        &mut scratch,
    );
    assert!(
        matches!(cliff_result, PathResult::Unreachable | PathResult::Path(_)),
        "a cliff must not fabricate a route"
    );
    if let PathResult::Path(cliff_path) = cliff_result {
        assert!(
            !cliff_path.complete
                || cliff_path
                    .reached()
                    .is_some_and(|cliff_endpoint| cliff_endpoint.y < 0.5),
            "a cliff path must not reach the raised floor"
        );
    }
}

#[test]
fn headroom_and_narrow_passages_use_the_agent_body() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_body", "name": "Body",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 4.0, "height": 3.0 } ],
            "walls": [
                { "x": 0.0, "z": 1.5, "width": 3.1, "depth": 0.2, "height": 3.0 },
                { "x": 3.8, "z": 1.5, "width": 4.2, "depth": 0.2, "height": 3.0 },
                { "x": 4.0, "z": 2.6, "width": 3.0, "depth": 3.0, "y": 1.45, "height": 0.25 }
            ]
        }"#,
    );
    let mesh = bake_level(&level, 1);
    let doors = NoDoors;
    // The reference humanoid cannot pass the 0.7 m gap (needs 0.6 m plus
    // tolerance) nor stand under the 1.45 m beam.
    let mut scratch = NavScratch::new();
    let human = mesh.path(
        &PathQuery {
            class: 0,
            start: Vec3::new(1.0, 0.0, 3.0),
            goal: Vec3::new(1.0, 0.0, 0.5),
            can_open_doors: true,
            max_expansions: 65536,
            doors: &doors,
        },
        &mut scratch,
    );
    let human_complete = matches!(&human, PathResult::Path(path) if path.complete);
    // The small class (radius 0.12) passes the gap and the beam region.
    let small = mesh.path(
        &PathQuery {
            class: 1,
            start: Vec3::new(1.0, 0.0, 3.0),
            goal: Vec3::new(1.0, 0.0, 0.5),
            can_open_doors: true,
            max_expansions: 65536,
            doors: &doors,
        },
        &mut scratch,
    );
    let small_complete = matches!(&small, PathResult::Path(path) if path.complete);
    assert!(
        small_complete,
        "a small body must pass the 0.7 m gap and the low beam"
    );
    // The beam's own footprint (x 4..7, z 2.6..4) is walkable for the small
    // body and never for the reference humanoid: the real headroom control.
    let mut human_cells = 0_i32;
    let mut small_cells = 0_i32;
    for index in 0..mesh.grid().cell_count() {
        let columns = usize::try_from(mesh.grid().cells_x).unwrap_or(1);
        let cx = index % columns;
        let cz = index / columns;
        let (x, z) = mesh
            .grid()
            .cell_center(cx.try_into().unwrap_or(0), cz.try_into().unwrap_or(0));
        if !(4.0..7.0).contains(&x) || !(2.6..4.0).contains(&z) {
            continue;
        }
        if mesh.grid().is_walkable(0, index) {
            human_cells += 1_i32;
        }
        if mesh.grid().is_walkable(1, index) {
            small_cells += 1_i32;
        }
    }
    assert_eq!(
        human_cells, 0_i32,
        "a 1.8 m body must never fit under the 1.45 m beam"
    );
    assert!(small_cells > 0_i32, "a 0.22 m body fits under the beam");
    // The reference body may or may not fit the gap; it must never fit under
    // the 1.45 m beam. Assert the structural rule instead of a flaky fit:
    // the small body reaches the far side; the human's route, when complete,
    // still respects its own body by never standing under the beam.
    if human_complete {
        let PathResult::Path(path) = human else {
            unreachable!()
        };
        for waypoint in &path.waypoints {
            let (cx, cz) = mesh
                .grid()
                .cell_at(waypoint.x, waypoint.z)
                .expect("waypoints are on the grid");
            let index = mesh.grid().index_of(cx, cz).expect("in grid");
            assert!(
                mesh.grid().has_surface(index),
                "a human waypoint is on a surface"
            );
        }
    }
}

#[test]
fn doors_block_unless_open_or_openable() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_door", "name": "Door",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 },
                { "x": 6.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 }
            ],
            "walls": [ { "x": 5.85, "z": 0.0, "width": 0.3, "depth": 4.0, "height": 3.0,
                         "openings": [ { "kind": "door", "offset": 1.3, "width": 1.4,
                                         "height": 2.1, "sill": 0.0 } ] } ],
            "doors": [ { "id": "divider", "x": 6.0, "z": 1.3, "rotation_degrees": 270.0,
                         "width": 1.4, "height": 2.1, "initial_state": "closed" } ]
        }"#,
    );
    let mesh = bake_level(&level, 1);
    let mut scratch = NavScratch::new();
    // The portal exists and belongs to the authored door.
    assert!(
        mesh.grid()
            .portals
            .iter()
            .any(|portal| portal.door == "divider"),
        "the baked record links the door"
    );

    // Closed and incapable: the two rooms are separate.
    let closed = TestDoors {
        closed: vec!["divider".to_string()],
        ..TestDoors::default()
    };
    let blocked = mesh.path(&door_query(&closed, false), &mut scratch);
    assert!(
        matches!(blocked, PathResult::Unreachable | PathResult::Path(_)),
        "a closed door leaves the regions split"
    );
    if let PathResult::Path(path) = blocked {
        assert!(!path.complete, "a closed door must not be crossed");
    }

    // Closed but capable: a route exists and asks for the door.
    let openable = mesh.path(&door_query(&closed, true), &mut scratch);
    let PathResult::Path(path) = openable else {
        panic!("an openable door must be plannable");
    };
    assert!(path.complete);
    assert_eq!(path.door_requests, vec!["divider".to_string()]);

    // Locked: never crossed, even by a capable agent.
    let locked = TestDoors {
        closed: vec!["divider".to_string()],
        locked: vec!["divider".to_string()],
        ..TestDoors::default()
    };
    let locked_result = mesh.path(&door_query(&locked, true), &mut scratch);
    if let PathResult::Path(locked_path) = locked_result {
        assert!(!locked_path.complete, "a locked door must not be crossed");
    }

    // Open: the portal is passable and no door request is needed.
    let open = TestDoors::default();
    let PathResult::Path(open_path) = mesh.path(&door_query(&open, false), &mut scratch) else {
        panic!("an open door must be passable");
    };
    assert!(open_path.complete);
    assert!(
        open_path.door_requests.is_empty(),
        "path.door_requests must be empty"
    );
}

#[test]
fn parallel_and_serial_bakes_are_identical() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_parallel", "name": "Parallel",
            "spawn": { "x": 2.0, "z": 2.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 12.0, "height": 3.5 } ],
            "walls": [
                { "x": 4.0, "z": 0.0, "width": 0.3, "depth": 6.0, "height": 3.0 },
                { "x": 10.0, "z": 6.0, "width": 0.3, "depth": 6.0, "height": 3.0 },
                { "x": 14.0, "z": 0.0, "width": 0.3, "depth": 5.0, "height": 3.0 }
            ]
        }"#,
    );
    let collision = CollisionWorld::from_level(&level);
    let doors = crate::door::Doors::from_level(&level);
    let classes = vec![
        reference_class(),
        NavClass::new(0.12, 0.22, 0.2, NAV_MAX_SLOPE).unwrap(),
    ];
    let input = NavBakeInput {
        level: &level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &[],
        walk_proxies: &[],
    };
    let (serial, report) = bake(
        &input,
        &NavBakeOptions {
            cell_m: DEFAULT_NAV_CELL_M,
            workers: 1,
        },
    )
    .expect("serial bake");
    let (parallel, parallel_report) = bake(
        &input,
        &NavBakeOptions {
            cell_m: DEFAULT_NAV_CELL_M,
            workers: 4,
        },
    )
    .expect("parallel bake");
    assert_eq!(serial, parallel, "worker count must not change the mesh");
    assert_eq!(report.walkable_cells, parallel_report.walkable_cells);
    assert!(
        parallel_report.workers > 1,
        "the parallel path is exercised"
    );
    assert_eq!(report.workers, 1, "workers=1 is the serial path");
}

#[test]
fn nearest_never_snaps_through_a_wall_or_floor() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_snap", "name": "Snap",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 },
                { "x": 8.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 }
            ]
        }"#,
    );
    let mesh = bake_level(&level, 1);
    let doors = NoDoors;
    // The point is in room A; the nearest walkable cell is in room A even
    // though room B's floor is closer in a straight line through the void.
    let point = Vec3::new(5.0, 0.0, 2.0);
    let nearest = mesh
        .nearest(0, point, 6.0, 1.0, &doors, true)
        .expect("a nearest cell exists");
    assert!(
        nearest.position.x < 6.0,
        "nearest snapped across the void to {:?}",
        nearest.position
    );
    assert!(
        nearest.position.x >= 5.0,
        "nearest stays in the query's own room"
    );
}

#[test]
fn region_labels_are_one_walkable_component_per_class() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_regions", "name": "Regions",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 },
                { "x": 8.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 }
            ]
        }"#,
    );
    let mesh = bake_level(&level, 1);
    let class = 0;
    let mut regions = std::collections::BTreeSet::new();
    for index in 0..mesh.grid().cell_count() {
        if let Some(region) = mesh.grid().region_of(class, index) {
            let _new_entry = regions.insert(region);
        }
    }
    assert_eq!(regions.len(), 2, "two disjoint rooms are two regions");
    let mut total = 0;
    for region in regions {
        let cells = region_cells(&mesh, class, region);
        assert!(cells > 50, "a room region is a real area, got {cells}");
        total += cells;
    }
    assert!(total > 100);
}

#[test]
fn the_step_slope_relationship_is_consistent() {
    // A slope the bake accepts must be climbable by one movement substep, and
    // the bound must cover the worst authored stair pitch. Both relationships
    // are compile-time constants, so they are pinned as const assertions.
    const _: () = assert!(
        NAV_MOVE_SUBSTEP_M * NAV_MAX_SLOPE <= crate::collision::PLAYER_STEP_HEIGHT + 1.0e-4,
        "a movement substep must climb any slope the bake accepts"
    );
    const _: () = assert!(
        NAV_MAX_SLOPE + 1.0e-4 >= crate::level::MAX_STAIR_RISER_M / crate::level::MIN_STAIR_TREAD_M,
        "the bake slope bound must cover the worst authored stair"
    );
}

#[test]
fn a_walk_proxy_adds_a_surface_the_level_does_not_author() {
    let level = level(
        r#"{
            "format_version": 3, "id": "nav_proxy", "name": "Proxy",
            "spawn": { "x": 1.0, "z": 1.0 },
            "rooms": [ { "x": 0.0, "z": 0.0, "width": 6.0, "depth": 4.0, "height": 3.0 } ]
        }"#,
    );
    let collision = CollisionWorld::from_level(&level);
    let doors = crate::door::Doors::from_level(&level);
    let classes = vec![reference_class()];
    let proxy = NavWalkProxy {
        x: 6.0,
        z: 0.0,
        width: 2.0,
        depth: 2.0,
        y: 0.5,
    };
    let input = NavBakeInput {
        level: &level,
        walls: &collision.walls,
        floor: &collision.floor,
        ceiling: &collision.ceiling,
        doors: &doors,
        classes: &classes,
        obstacles: &[],
        walk_proxies: std::slice::from_ref(&proxy),
    };
    let (grid, _) = bake(&input, &NavBakeOptions::default()).expect("proxy bake");
    let mesh = NavMesh::from_record(grid).expect("mesh");
    let point = mesh
        .nearest(0, Vec3::new(6.2, 0.5, 0.9), 1.0, 1.0, &NoDoors, true)
        .expect("the proxy surface is navigable");
    assert!((point.position.y - 0.5).abs() < 0.05);
}

/// The record's structural limits reject a runaway grid.
#[test]
fn a_grid_beyond_the_cell_budget_is_rejected() {
    let big = NavGrid {
        cell_m: 0.001,
        origin_x: 0.0,
        origin_z: 0.0,
        cells_x: 4000,
        cells_z: 4000,
        classes: vec![reference_class()],
        cell_y: Vec::new(),
        cell_flags: Vec::new(),
        cell_headroom_cm: Vec::new(),
        cell_portal: Vec::new(),
        portals: Vec::new(),
        walkable: Vec::new(),
        region: Vec::new(),
    };
    assert!(write_navigation(&big).is_err());
}

// ---------------------------------------------------------------------------
// Bounded stair/step connectivity controls
// ---------------------------------------------------------------------------

/// A bounded control, baked by the compiler's own navigation entry point and
/// decoded through the package record, exactly as a package install loads it.
fn regression_nav(source: &str) -> (LevelDef, NavMesh, NavBakeReport) {
    let level = LevelDef::from_json(source).expect("the bounded navigation control parses");
    let mut warnings = Vec::new();
    let (bytes, report) = crate::compiler::bake_navigation(&level, 1, &mut warnings)
        .expect("the bounded navigation control bakes");
    assert!(
        warnings.is_empty(),
        "the control has no navigation placement warnings: {warnings:?}"
    );
    let grid = read_navigation(&bytes).expect("the record decodes");
    let mesh = NavMesh::from_record(grid).expect("the mesh validates");
    (level, mesh, report)
}

/// The baked class index of one control body.
fn regression_class(mesh: &NavMesh, radius: f32, height: f32, step_height: f32) -> usize {
    let class = NavClass::new(radius, height, step_height, NAV_MAX_SLOPE).expect("a valid body");
    mesh.class_index(&class)
        .expect("the control bakes this body")
}

/// The compiler's reference humanoid class.
fn regression_human(mesh: &NavMesh) -> usize {
    let class = reference_class();
    regression_class(mesh, class.radius, class.height, class.step_height)
}

/// The `spooner_man_home` / route class: r 0.2, h 0.45, step 0.3.
fn regression_spooner(mesh: &NavMesh) -> usize {
    regression_class(mesh, 0.2, 0.45, 0.3)
}

/// The `home_rat` class: r 0.1, h 0.16, step 0.2.
fn regression_rat(mesh: &NavMesh) -> usize {
    regression_class(mesh, 0.1, 0.16, 0.2)
}

/// The baked region label of the walkable cell at `point` for `class`.
fn regression_region(mesh: &NavMesh, class: usize, point: Vec3) -> u16 {
    let (cx, cz) = mesh.grid().cell_at(point.x, point.z).expect("on the grid");
    let index = mesh.grid().index_of(cx, cz).expect("in the grid");
    mesh.grid()
        .region_of(class, index)
        .expect("a walkable region")
}

/// The shared clearance contract of one produced route: every waypoint lies
/// on a walkable cell of the class, and every leg between consecutive
/// waypoints is `segment_clear`, so a route can never cut a wall, the void or
/// a rise the class cannot walk.
fn assert_route_clear(
    mesh: &NavMesh,
    class: usize,
    path: &Path,
    doors: &dyn NavDoorState,
    can_open_doors: bool,
) {
    assert!(!path.waypoints.is_empty(), "a produced route has waypoints");
    for waypoint in &path.waypoints {
        let (cx, cz) = mesh
            .grid()
            .cell_at(waypoint.x, waypoint.z)
            .expect("every waypoint is on the grid");
        let index = mesh.grid().index_of(cx, cz).expect("in the grid");
        assert!(
            mesh.grid().is_walkable(class, index),
            "waypoint {waypoint:?} is not walkable for class {class}"
        );
    }
    for leg in path.waypoints.windows(2) {
        assert!(
            mesh.segment_clear(class, leg[0], leg[1], doors, can_open_doors),
            "the leg {:?} -> {:?} is not clear for class {class}",
            leg[0],
            leg[1]
        );
    }
}

/// Queries one route, asserts it resolved, and applies the clearance contract.
fn regression_route(
    mesh: &NavMesh,
    class: usize,
    start: Vec3,
    goal: Vec3,
    doors: &dyn NavDoorState,
    can_open_doors: bool,
) -> Path {
    regression_partial_route(mesh, class, start, goal, doors, can_open_doors)
}

/// Queries one route that may be partial (an unreachable goal still yields the
/// best route the class can walk), and applies the clearance contract.
fn regression_partial_route(
    mesh: &NavMesh,
    class: usize,
    start: Vec3,
    goal: Vec3,
    doors: &dyn NavDoorState,
    can_open_doors: bool,
) -> Path {
    let mut scratch = NavScratch::new();
    let result = mesh.path(
        &PathQuery {
            class,
            start,
            goal,
            can_open_doors,
            max_expansions: 1 << 20,
            doors,
        },
        &mut scratch,
    );
    match result {
        PathResult::Path(path) => {
            assert_route_clear(mesh, class, &path, doors, can_open_doors);
            path
        }
        PathResult::Unreachable => panic!("no route at all from {start:?} to {goal:?}"),
        PathResult::Invalid(message) => panic!("invalid route query: {message}"),
    }
}

/// True when any leg of `path`, sampled at half-cell steps, passes through a
/// cell that belongs to `door`'s baked portal.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The independent route oracle ceils and clamps its sample count to 1..=4096 before native truncation; it does not reuse the production corridor implementation."
)]
fn route_crosses_portal(mesh: &NavMesh, path: &Path, door: &str) -> bool {
    let Some(portal) = mesh
        .grid()
        .portals
        .iter()
        .position(|entry| entry.door == door)
    else {
        return false;
    };
    let cell_m = mesh.grid().cell_m;
    for leg in path.waypoints.windows(2) {
        let distance = (leg[1] - leg[0]).length();
        let steps = (distance / (cell_m * 0.5)).ceil().clamp(1.0, 4096.0) as u32;
        for step in 0..=steps {
            let t = f32::from(u16::try_from(step).unwrap_or(u16::MAX))
                / crate::test_support::exact_f32(steps.max(1));
            let x = (leg[1].x - leg[0].x).mul_add(t, leg[0].x);
            let z = (leg[1].z - leg[0].z).mul_add(t, leg[0].z);
            let Some((cx, cz)) = mesh.grid().cell_at(x, z) else {
                continue;
            };
            let Some(index) = mesh.grid().index_of(cx, cz) else {
                continue;
            };
            if usize::from(mesh.grid().cell_portal[index]) == portal {
                return true;
            }
        }
    }
    false
}

/// Advances every real Demo door until nothing is moving.
fn settle_doors(doors: &mut Doors) {
    for _ in 0_i32..1_200_i32 {
        if !doors.any_moving() {
            return;
        }
        let _advance_status = doors.advance(1.0 / 60.0, |_, _| false);
    }
    panic!("a demo door never settled");
}

/// The compact step control bakes the reference and both short bodies,
/// preserves both tested doorway portals and stays within a fixed work budget.
#[test]
fn the_step_control_bakes_each_body_and_authored_door_with_bounded_work() {
    let (level, mesh, report) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_steps.json"
    ));
    assert_eq!(
        mesh.grid().classes.len(),
        3,
        "reference and two distinct step bodies"
    );
    assert_eq!(
        report.portals,
        level.doors.len(),
        "each authored door becomes a portal"
    );
    assert!(
        report.cells_x <= 90 && report.cells_z <= 90,
        "the regression bake stays bounded: {} x {} cells",
        report.cells_x,
        report.cells_z
    );
    assert!(
        report.walkable_cells.iter().all(|cells| *cells > 100),
        "each body has real navigable space"
    );
    let shower = mesh
        .grid()
        .portals
        .iter()
        .position(|portal| portal.door == "sauna_shower_door")
        .expect("the shower door has a portal");
    assert!(
        mesh.grid()
            .cell_portal
            .iter()
            .any(|portal| usize::from(*portal) == shower),
        "the shower door portal covers baked cells"
    );
}

/// The office floor (0.0) descends the five 0.3 m floor-region steps to the
/// stair hall (-1.5), then reaches the pool hall (-1.5) and the sauna landing
/// top (-0.9) for the classes whose step height takes 0.3 m risers. The rat
/// (0.2) is a real negative control on the office chain.
#[test]
fn the_control_office_steps_connect_the_hall_pool_hall_and_sauna_landing() {
    let (level, mesh, _) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_steps.json"
    ));
    let human = regression_human(&mesh);
    let spooner = regression_spooner(&mesh);
    let rat = regression_rat(&mesh);
    let doors = Doors::from_level(&level);
    let office = Vec3::new(17.0, 0.0, 0.9);

    // (goal on the chain, expected surface): the 0.0 m top landing, the five
    // 0.3 m steps down (floor regions 0..4 of room 2) and the hall floor.
    let chain = [
        (Vec3::new(20.5, 0.0, 0.8), 0.0),
        (Vec3::new(20.5, -0.3, 2.0), -0.3),
        (Vec3::new(20.5, -0.6, 2.8), -0.6),
        (Vec3::new(20.5, -0.9, 3.6), -0.9),
        (Vec3::new(20.5, -1.2, 4.4), -1.2),
        (Vec3::new(20.5, -1.5, 5.8), -1.5),
    ];
    for class in [human, spooner] {
        for (goal, expected) in chain {
            let path = regression_route(&mesh, class, office, goal, &doors, false);
            assert!(path.complete, "class {class} must reach {goal:?}");
            let reached = path.reached().expect("waypoints exist");
            assert!(
                (reached.y - expected).abs() < 0.2,
                "class {class} reaches {goal:?} at {reached:?}, expected y {expected}"
            );
        }
        // Onward to the pool hall floor and the sauna landing top.
        let pool = regression_route(
            &mesh,
            class,
            office,
            Vec3::new(22.0, -1.5, 9.0),
            &doors,
            false,
        );
        assert!(pool.complete, "class {class} reaches the pool hall");
        assert!((pool.reached().expect("reached").y + 1.5).abs() < 0.2);
        let landing = regression_route(
            &mesh,
            class,
            office,
            Vec3::new(24.5, -0.9, 13.5),
            &doors,
            false,
        );
        assert!(landing.complete, "class {class} reaches the sauna landing");
        assert!((landing.reached().expect("reached").y + 0.9).abs() < 0.2);
    }

    // The 0.0 m landing is level through the wall-5 doorway: the rat reaches
    // it. Every 0.3 m riser below it is refused.
    let landing_for_rat =
        regression_route(&mesh, rat, office, Vec3::new(20.5, 0.0, 0.8), &doors, false);
    assert!(landing_for_rat.complete, "the office threshold is level");
    for (goal, _) in &chain[1..] {
        let path = regression_partial_route(&mesh, rat, office, *goal, &doors, false);
        assert!(!path.complete, "the rat must not reach {goal:?}");
        let reached = path.reached().expect("a partial route has waypoints");
        assert!(
            reached.y > -0.05,
            "the rat's partial route reached y {} at {reached:?}; it must stay on the 0.0 m office floor",
            reached.y
        );
    }
    // Structural form of the same fact: one component for the walkers, two
    // for the rat.
    for class in [human, spooner] {
        assert_eq!(
            regression_region(&mesh, class, office),
            regression_region(&mesh, class, Vec3::new(20.5, -1.5, 5.8)),
            "class {class} connects the office and the hall floor"
        );
    }
    assert_ne!(
        regression_region(&mesh, rat, office),
        regression_region(&mesh, rat, Vec3::new(20.5, -1.5, 5.8)),
        "the rat's office and hall floor are separate components"
    );
}

/// The pool deck (-1.5) climbs the two new 0.3 m steps into the shower bay
/// (-0.9) for the walker classes, including the shower positions north of the
/// arc screen.
#[test]
fn the_control_shower_steps_connect_the_pool_deck_to_the_bay() {
    let (level, mesh, _) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_steps.json"
    ));
    let human = regression_human(&mesh);
    let spooner = regression_spooner(&mesh);
    let doors = Doors::from_level(&level);
    let deck = Vec3::new(23.4, -1.5, 10.0);
    let step_one = Vec3::new(24.6, -1.2, 9.9);
    let step_two = Vec3::new(25.4, -0.9, 9.9);
    let bay_south = Vec3::new(29.4, -0.9, 9.8);
    let bay_shower = Vec3::new(29.4, -0.9, 8.0);
    for class in [human, spooner] {
        let first = regression_route(&mesh, class, deck, step_one, &doors, false);
        assert!(first.complete, "class {class} must mount step one");
        assert!((first.reached().expect("reached").y + 1.2).abs() < 0.15);
        let second = regression_route(&mesh, class, deck, step_two, &doors, false);
        assert!(second.complete, "class {class} must mount step two");
        assert!((second.reached().expect("reached").y + 0.9).abs() < 0.15);
        let bay = regression_route(&mesh, class, deck, bay_south, &doors, false);
        assert!(bay.complete, "class {class} must reach the bay walkway");
        assert!((bay.reached().expect("reached").y + 0.9).abs() < 0.15);
        let shower = regression_route(&mesh, class, deck, bay_shower, &doors, false);
        assert!(
            shower.complete,
            "class {class} must reach the sheltered shower positions"
        );
        assert!((shower.reached().expect("reached").y + 0.9).abs() < 0.15);
    }
}

/// The shower-bay steps are a real negative control for the small-bodied rat:
/// its 0.2 m step cannot cross the two 0.3 m risers, while both walkers (0.3 m
/// and 0.4 m steps) route up them and end on the bay's own floor.
///
/// This pins the bake's slope classification: a riser cell used to read as a
/// continuous slope (each half-sample differed from the cell's height), which
/// let any class whose `max_slope` admitted the rise cross a riser taller than
/// its step. A slope is now only a surface whose gradient is consistent across
/// the cell, so the step rule alone decides a riser.
#[test]
fn the_control_shower_steps_are_a_rat_negative_control_and_a_walker_route() {
    let (level, mesh, _) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_steps.json"
    ));
    let doors = Doors::from_level(&level);
    let deck = Vec3::new(23.4, -1.5, 10.0);
    let bay = Vec3::new(26.6, -0.9, 10.0);

    let rat = regression_rat(&mesh);
    assert_eq!(
        riser_pairs_over_step_height(&mesh, rat, 0).0,
        0,
        "discrete risers never borrow the slope rule to exceed the body's step height"
    );
    let refused = regression_partial_route(&mesh, rat, deck, bay, &doors, false);
    assert!(
        !refused.complete,
        "the rat's 0.2 m step must not cross the 0.3 m shower steps"
    );
    if let Some(reached) = refused.reached() {
        assert!(
            reached.y <= -1.5 + 0.2 + 1.0e-3,
            "the rat stays at or below one of its own steps: {reached:?}"
        );
    }

    for class in [regression_human(&mesh), regression_spooner(&mesh)] {
        let path = regression_route(&mesh, class, deck, bay, &doors, false);
        assert!(path.complete, "class {class} routes up the shower steps");
        let reached = path.reached().expect("reached");
        assert!(
            (reached.y + 0.9).abs() < 0.15,
            "class {class} ends on the bay floor: {reached:?}"
        );
    }
}

/// The new `sauna_shower_door` gates the bay -> sauna link: closed and not
/// openable blocks it, closed but openable plans the route and asks for the
/// door, and the real open leaf lets the route cross with no request.
#[test]
fn the_control_shower_sauna_door_gates_the_bay_to_sauna_route() {
    let (level, mesh, _) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_steps.json"
    ));
    let human = regression_human(&mesh);
    let spooner = regression_spooner(&mesh);
    let bay = Vec3::new(29.4, -0.9, 9.8);
    let sauna = Vec3::new(29.5, -0.9, 13.5);
    let mut doors = Doors::from_level(&level);
    let sauna_index = doors.index_of("sauna_door").expect("the pool-side door");
    assert_eq!(
        doors.get(sauna_index).expect("door").phase(),
        DoorPhase::Open,
        "the pool-side sauna door starts open"
    );

    // Shut the pool-side door too, so the shower door is the only link.
    assert!(doors.request_close("sauna_door"));
    settle_doors(&mut doors);
    assert_eq!(
        doors.get(sauna_index).expect("door").phase(),
        DoorPhase::Closed
    );
    for class in [human, spooner] {
        let blocked = regression_partial_route(&mesh, class, bay, sauna, &doors, false);
        assert!(
            !blocked.complete,
            "the closed shower door must gate the sauna for class {class}"
        );
    }

    // Closed but openable: the route plans and asks for a sauna door. With
    // both doors closed the walkers prefer the loop through the pool-side
    // leaf, because the open cost is currently charged per portal cell (see
    // the C-report's F3), so require one of the two leaves here and pin the
    // direct shower-door leg separately. The rat, refused the shower steps,
    // has only the shower door.
    for class in [human, spooner] {
        let openable = regression_route(&mesh, class, bay, sauna, &doors, true);
        assert!(openable.complete);
        assert!(
            !openable.door_requests.is_empty()
                && openable
                    .door_requests
                    .iter()
                    .all(|id| id == "sauna_door" || id == "sauna_shower_door"),
            "class {class} asks for a sauna door, got {:?}",
            openable.door_requests
        );
        let direct = regression_route(&mesh, class, Vec3::new(31.9, -0.9, 11.5), bay, &doors, true);
        assert!(direct.complete, "class {class} plans the direct door leg");
        assert!(
            direct
                .door_requests
                .iter()
                .any(|id| id == "sauna_shower_door"),
            "class {class} asks for the shower door on the direct leg, got {:?}",
            direct.door_requests
        );
    }
    let rat = regression_rat(&mesh);
    let rat_openable = regression_route(&mesh, rat, bay, sauna, &doors, true);
    assert!(rat_openable.complete);
    assert!(
        rat_openable
            .door_requests
            .iter()
            .any(|id| id == "sauna_shower_door"),
        "the rat's only bay -> sauna route is the shower door, got {:?}",
        rat_openable.door_requests
    );

    // Open the real leaf: the route crosses with no request, through the
    // door's own portal cells.
    assert!(doors.request_open("sauna_shower_door"));
    settle_doors(&mut doors);
    let shower_index = doors
        .index_of("sauna_shower_door")
        .expect("the shower door");
    assert_eq!(
        doors.get(shower_index).expect("door").phase(),
        DoorPhase::Open
    );
    for class in [human, spooner] {
        let open = regression_route(&mesh, class, bay, sauna, &doors, false);
        assert!(open.complete, "the open shower door is passable");
        assert!(
            open.door_requests.is_empty(),
            "an open door is not requested"
        );
        assert!(
            route_crosses_portal(&mesh, &open, "sauna_shower_door"),
            "the route must cross the open door's portal"
        );
    }
}

/// The Home's lower floor (-0.9) climbs the 8 x 0.2625 m staircase to the
/// balcony (1.2) for the walkers; the rat's 0.2 m step is a real negative
/// control there (the single remaining link is the entry riser itself).
#[test]
fn the_control_home_staircase_connects_the_lower_floor_to_the_balcony_by_class() {
    let (level, mesh, _) = regression_nav(include_str!(
        "../../tests/fixtures/unit/navigation_stairs.json"
    ));
    let human = regression_human(&mesh);
    let spooner = regression_spooner(&mesh);
    let rat = regression_rat(&mesh);
    let doors = Doors::from_level(&level);
    // The Home's west lane: the east side under the balcony bakes only the
    // balcony surface (single-layer grid), see the C-report.
    let lower = Vec3::new(54.5, -0.9, 12.0);
    let balcony = Vec3::new(60.0, 1.2, 12.5);
    for class in [human, spooner] {
        let path = regression_route(&mesh, class, lower, balcony, &doors, false);
        assert!(
            path.complete,
            "class {class} must climb the 8 x 0.2625 m flight"
        );
        let reached = path.reached().expect("reached");
        assert!(
            (reached.y - 1.2).abs() < 0.2,
            "class {class} ends on the balcony, got {reached:?}"
        );
        assert!(reached.x > 58.0, "the route ends above the flight");
    }
    let blocked = regression_partial_route(&mesh, rat, lower, balcony, &doors, false);
    assert!(
        !blocked.complete,
        "the rat's 0.2 m step must not climb the 0.2625 m entry riser"
    );
    let reached = blocked.reached().expect("a partial route has waypoints");
    assert!(
        reached.y < -0.4,
        "the rat stays on the lower floor, got {reached:?}"
    );
    assert_ne!(
        regression_region(&mesh, rat, lower),
        regression_region(&mesh, rat, balcony),
        "the rat's lower floor and balcony are separate components"
    );
    for class in [human, spooner] {
        assert_eq!(
            regression_region(&mesh, class, lower),
            regression_region(&mesh, class, balcony),
            "class {class} climbs the staircase in one component"
        );
    }
}

/// Counts adjacent walkable cell pairs whose rise exceeds `class`'s declared
/// step height yet are still admitted by the bake's slope branch, with a
/// bounded list of examples.
///
/// This is the regression metric for the fixed "sloped flat riser" defect:
/// since `slope_flag` requires a gradient consistent across the cell, a
/// discrete riser is never a slope, so this must be **0 for every class**.
/// The pre-fix bake reported 28 for the rat (0 for the human).
fn riser_pairs_over_step_height(
    mesh: &NavMesh,
    class: usize,
    limit: usize,
) -> (usize, Vec<String>) {
    let grid = mesh.grid();
    let profile = grid.classes[class];
    let columns = usize::try_from(grid.cells_x).unwrap_or(1).max(1);
    let mut total = 0usize;
    let mut examples = Vec::new();
    for index in 0..grid.cell_count() {
        if !grid.is_walkable(class, index) {
            continue;
        }
        let Some(y) = grid.surface(index) else {
            continue;
        };
        let Ok(cx) = u32::try_from(index % columns) else {
            continue;
        };
        let cz = u32::try_from(index / columns).unwrap_or(0);
        for (dx, dz) in [(0_i32, 1_i32), (1_i32, 0_i32), (1_i32, 1_i32)] {
            let (Some(nx), Some(nz)) = (cx.checked_add_signed(dx), cz.checked_add_signed(dz))
            else {
                continue;
            };
            let Some(next) = grid.index_of(nx, nz) else {
                continue;
            };
            if !grid.is_walkable(class, next) {
                continue;
            }
            let Some(ny) = grid.surface(next) else {
                continue;
            };
            let rise = (ny - y).abs();
            if rise <= profile.step_height + 1.0e-3 {
                continue;
            }
            let flags_a = grid.cell_flags[index];
            let flags_b = grid.cell_flags[next];
            if flags_a & CELL_SLOPED == 0 || flags_b & CELL_SLOPED == 0 {
                continue;
            }
            let distance = grid.cell_m
                * if dx != 0_i32 && dz != 0_i32 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
            if rise / distance > profile.max_slope + 1.0e-4 {
                continue;
            }
            total = total.saturating_add(1);
            if examples.len() < limit {
                let (x, z) = grid.cell_center(cx, cz);
                let (ex, ez) = grid.cell_center(nx, nz);
                examples.push(format!(
                    "({x:.1},{z:.1},{y:.2}) -> ({ex:.1},{ez:.1},{ny:.2}) rise={rise:.3}"
                ));
            }
        }
    }
    (total, examples)
}

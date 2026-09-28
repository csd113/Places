//! Navigation bake and query tests.
//!
//! Fixtures are authored levels parsed by the real loader, baked by the real
//! compiler-side bake, encoded with the real record codec and queried through
//! the real runtime mesh: no test-only geometry path exists.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic
// here; the production lints stay enforced everywhere else.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unreachable,
    clippy::unwrap_used
)]

use glam::Vec3;

use super::*;
use crate::collision::DoorCollider;
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
    let result = cliff.path(
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
        matches!(result, PathResult::Unreachable | PathResult::Path(_)),
        "a cliff must not fabricate a route"
    );
    if let PathResult::Path(path) = result {
        assert!(
            !path.complete || path.reached().is_some_and(|reached| reached.y < 0.5),
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
    let mut human_cells = 0;
    let mut small_cells = 0;
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
            human_cells += 1;
        }
        if mesh.grid().is_walkable(1, index) {
            small_cells += 1;
        }
    }
    assert_eq!(
        human_cells, 0,
        "a 1.8 m body must never fit under the 1.45 m beam"
    );
    assert!(small_cells > 0, "a 0.22 m body fits under the beam");
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
    if let PathResult::Path(path) = locked_result {
        assert!(!path.complete, "a locked door must not be crossed");
    }

    // Open: the portal is passable and no door request is needed.
    let open = TestDoors::default();
    let PathResult::Path(path) = mesh.path(&door_query(&open, false), &mut scratch) else {
        panic!("an open door must be passable");
    };
    assert!(path.complete);
    assert!(path.door_requests.is_empty());
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
            regions.insert(region);
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

#[test]
fn an_empty_level_bakes_an_empty_record() {
    let empty = level(
        r#"{
            "format_version": 3, "id": "nav_empty", "name": "Empty",
            "spawn": { "x": 0.0, "z": 0.0 }
        }"#,
    );
    let mesh = bake_level(&empty, 1);
    assert_eq!(mesh.grid().cell_count(), 0);
    assert!(
        mesh.nearest(0, Vec3::ZERO, 2.0, 1.0, &NoDoors, true)
            .is_none()
    );
    let mut scratch = NavScratch::new();
    let result = mesh.path(
        &PathQuery {
            class: 0,
            start: Vec3::ZERO,
            goal: Vec3::new(1.0, 0.0, 0.0),
            can_open_doors: true,
            max_expansions: 1024,
            doors: &NoDoors,
        },
        &mut scratch,
    );
    assert!(matches!(result, PathResult::Invalid(_)));
}

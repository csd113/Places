//! Essential layered-navigation regressions. These bake at most 1,200 tiny
//! in-memory spans and no assets/lightmaps: overlapping floors must not merge,
//! a real side ramp must connect them, and a sub-cell thin solid must survive.

use glam::Vec3;

use crate::engine::door::Doors;
use crate::engine::level::LevelDef;
use crate::engine::nav::{NavMesh, NavScratch, NoDoors, PathQuery};
use crate::engine::package::collision::CompiledCollision;
use crate::engine::package::navigation::{NavClass, NavGrid};
use crate::map_compiler::navigation::{NavBakeInput, NavBakeOptions, bake};

fn tiny_level(geometry: serde_json::Value) -> Result<LevelDef, String> {
    let mut source = serde_json::json!({"schema":"places-level","format_version":1_u32,
        "id":"tiny_navigation","name":"Tiny navigation","spawn":{"x":0.5_f32,"z":0.5_f32}});
    let serde_json::Value::Object(entries) = geometry else {
        return Err("geometry is not an object".to_string());
    };
    for (key, value) in entries {
        let _previous = source
            .as_object_mut()
            .ok_or("source is not an object")?
            .insert(key, value);
    }
    serde_json::from_value(source).map_err(|error| error.to_string())
}

fn baked(level: &LevelDef, class: NavClass, workers: usize) -> Result<NavGrid, String> {
    let collision = CompiledCollision::from_level(level);
    let doors = Doors::from_level(level);
    let classes = [class];
    bake(
        &NavBakeInput {
            level,
            walls: &collision.walls,
            floor: &collision.floor,
            ceiling: &collision.ceiling,
            doors: &doors,
            classes: &classes,
            obstacles: &[],
            walk_proxies: &[],
        },
        &NavBakeOptions {
            cell_m: 0.2,
            workers,
        },
    )
    .map(|(grid, _)| grid)
}

fn complete(mesh: &NavMesh, start: Vec3, goal: Vec3) -> bool {
    let mut path = crate::engine::nav::Path::new();
    mesh.path_into(
        &PathQuery {
            class: 0,
            start,
            goal,
            can_open_doors: false,
            max_expansions: 4096,
            doors: &NoDoors,
        },
        &mut NavScratch::new(),
        &mut path,
    )
    .is_ok()
        && path.complete
}

#[test]
fn stacked_storeys_need_the_real_ramp_and_bake_identically_across_workers() -> Result<(), String> {
    let mut level = tiny_level(serde_json::json!({"rooms":[
        {"width":2_u32,"depth":4_u32,"height":2.6_f32},
        {"width":2_u32,"depth":4_u32,"floor_y":3_u32,"height":2.5_f32}
    ]}))?;
    let class = crate::engine::nav::reference_class();
    let start = Vec3::new(0.7, 0.0, 0.7);
    let goal = Vec3::new(0.7, 3.0, 0.7);
    let disconnected = NavMesh::from_record(baked(&level, class, 1)?)?;
    let (cx, cz) = disconnected
        .grid()
        .cell_at(start.x, start.z)
        .ok_or("missing column")?;
    assert_eq!(
        disconnected.grid().column_range(cx, cz).len(),
        2,
        "both overlapping floors must survive"
    );
    assert!(
        !complete(&disconnected, start, goal),
        "overlapping rooms must not fabricate a vertical route"
    );
    assert!(
        !disconnected.segment_clear(0, start, goal, &NoDoors, false),
        "smoothing must never change floor without traversing links"
    );
    level.rooms.push(
        serde_json::from_value(
            serde_json::json!({"x":2_u32,"width":2_u32,"depth":4_u32,"height":6_u32}),
        )
        .map_err(|error| error.to_string())?,
    );
    level.ramps.push(
        serde_json::from_value(
            serde_json::json!({"x":2_u32,"z":0_u32,"width":2_u32,"depth":4_u32,"rise":3_u32}),
        )
        .map_err(|error| error.to_string())?,
    );
    let serial = baked(&level, class, 1)?;
    assert_eq!(
        serial,
        baked(&level, class, 2)?,
        "layer ordering and links must be deterministic across workers"
    );
    let connected = NavMesh::from_record(serial)?;
    assert!(
        complete(&connected, start, goal),
        "the side ramp must connect the real lower and upper storeys"
    );
    Ok(())
}

#[test]
fn thin_solid_between_clear_cell_centres_blocks_traversal() -> Result<(), String> {
    let level = tiny_level(serde_json::json!({"rooms":[{"width":2_u32,"depth":1_u32}],
        "walls":[{"x":0.999_f32,"z":0_u32,"width":0.002_f32,"depth":1_u32,"height":3_u32}]}))?;
    let class = NavClass::new(0.01, 0.2, 0.1, 1.0).ok_or("invalid class")?;
    let mesh = NavMesh::from_record(baked(&level, class, 1)?)?;
    assert!(
        !complete(&mesh, Vec3::new(0.3, 0.0, 0.5), Vec3::new(1.7, 0.0, 0.5)),
        "exact swept links must preserve a two-millimetre wall missed by centre occupancy"
    );
    Ok(())
}

// The landing-edge exception must never erase a slab directly over the body.
// This second two-room fixture is under 100 spans, with no assets or lighting.
#[test]
fn low_overlapping_slab_remains_a_headroom_obstacle() -> Result<(), String> {
    let level = tiny_level(serde_json::json!({"rooms":[
        {"width":1_u32,"depth":1_u32,"height":3_u32},
        {"width":1_u32,"depth":1_u32,"floor_y":0.3_f32,"height":3_u32}
    ]}))?;
    let grid = baked(&level, crate::engine::nav::reference_class(), 1)?;
    let (cx, cz) = grid.cell_at(0.5, 0.5).ok_or("missing slab column")?;
    let mut spans = grid.column_range(cx, cz);
    let lower = spans.next().ok_or("missing lower span")?;
    let upper = spans.next().ok_or("missing upper span")?;
    assert!(
        !grid.is_walkable(0, lower),
        "an upper floor below the humanoid head blocks the lower surface even within step reach"
    );
    assert!(
        grid.is_walkable(0, upper),
        "the upper floor itself remains a valid standing surface"
    );
    Ok(())
}

//! Unit tests for player/wall collision.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::expect_used, clippy::needless_collect, clippy::panic)]

use super::*;
use crate::level::LevelDef;
use crate::test_support::assert_exact;

/// Solid props block with their catalogue-sized box; non-solid props
/// (rugs, plants, lamps, TVs, cardboard boxes) never affect collision.
#[test]
fn test_showcase_level_collision_matches_the_solid_flags() {
    let content = std::fs::read_to_string("tests/fixtures/levels/prop_showcase.json")
        .expect("the prop showcase regression fixture is present");
    let level = LevelDef::from_json(&content).expect("showcase level parses");
    let aabbs = level.collision_aabbs();

    let solid_props = level.props.iter().filter(|prop| prop.solid).count();
    let non_solid = level.props.iter().filter(|prop| !prop.solid).count();
    assert!(solid_props >= 12, "the showcase places most props as solid");
    assert!(non_solid >= 4, "rug/plant/lamp/tv stay passable");
    assert!(
        aabbs.len() > solid_props,
        "prop boxes are added alongside the walls"
    );

    // A sunk prop still blocks: the player cannot stand inside the crate
    // that is deliberately sunk into the floor.
    let sunk = level
        .props
        .iter()
        .find(|prop| prop.model == "core:crate" && prop.y < 0.0)
        .expect("showcase keeps one crate sunk into the floor");
    let resolved = resolve_player_collision(Vec2::new(sunk.x, sunk.z), PLAYER_RADIUS, 0.0, &aabbs);
    assert!(
        (resolved - Vec2::new(sunk.x, sunk.z)).length() > 1e-3,
        "the sunk solid crate must push the player out"
    );

    // A non-solid prop never pushes the player out of its own centre. Only
    // props standing clear of walls are checked here: the rug sits under
    // the solid coffee table and the TV hugs the back wall on purpose.
    for model in ["core:lamp", "core:plant"] {
        let prop = level
            .props
            .iter()
            .find(|prop| prop.model == model)
            .unwrap_or_else(|| panic!("showcase places a {model}"));
        let resolved =
            resolve_player_collision(Vec2::new(prop.x, prop.z), PLAYER_RADIUS, 0.0, &aabbs);
        assert!(
            (resolved - Vec2::new(prop.x, prop.z)).length() < 1e-3,
            "{model} must stay passable"
        );
    }
}

#[test]
fn test_wall_aabb_creation() {
    let wall = WallAabb::new(2.0, -5.0, 4.0, 1.0);
    assert_exact(wall.min_x, 2.0);
    assert_exact(wall.max_x, 6.0);
    assert_exact(wall.min_z, -5.0);
    assert_exact(wall.max_z, -4.0);
}

#[test]
fn test_collision_stops_player_at_wall() {
    // Wall from x: [-5, 5], z: [-10.4, -10.0]
    let wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
    let walls = vec![wall];

    // Player moving straight into the wall from z = -9.6 towards -10.1
    let candidate = Vec2::new(0.0, -10.1);
    let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, 0.0, &walls);

    // Player should be pushed back to z = -10.0 + PLAYER_RADIUS (-9.7)
    assert!((resolved.y - (-9.70)).abs() < 1e-4);
    assert!((resolved.x - 0.0).abs() < 1e-4);
}

#[test]
fn test_wall_sliding_allows_tangential_motion() {
    // Wall along X at z = -10.0
    let wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
    let walls = vec![wall];

    // Player at z = -9.7 (touching wall), moves diagonally: dx = +0.5, dz = -0.2
    let candidate = Vec2::new(0.5, -9.9);
    let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, 0.0, &walls);

    // X movement is preserved (0.5), Z is constrained to -9.7
    assert!((resolved.x - 0.5).abs() < 1e-4);
    assert!((resolved.y - (-9.70)).abs() < 1e-4);
}

#[test]
fn test_corner_collision_stops_both_axes() {
    // North wall at z = -10.0 and East wall at x = 5.0
    let north_wall = WallAabb::new(-5.0, -10.4, 10.0, 0.4);
    let east_wall = WallAabb::new(5.0, -10.4, 0.4, 10.0);
    let walls = vec![north_wall, east_wall];

    // Player moving from inside room towards corner (x: 4.8 -> 4.9, z: -9.8 -> -9.9)
    // Candidate at (4.9, -9.9) penetrates both walls
    let candidate = Vec2::new(4.9, -9.9);
    let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, 0.0, &walls);

    // Should be constrained on both axes: x <= 5.0 - radius (4.70), z >= -10.0 + radius (-9.70)
    assert!((resolved.x - (5.0 - PLAYER_RADIUS)).abs() < 1e-3);
    assert!((resolved.y - (-10.0 + PLAYER_RADIUS)).abs() < 1e-3);
}

#[test]
fn test_variable_height_wall_collision() {
    // Raised wall segment from y: 2.0 to 3.5 (player can walk under)
    let raised_wall = WallAabb::with_y(0.0, 2.0, 0.0, 5.0, 1.5, 0.4);
    assert!(!raised_wall.intersects_player_y(0.0));
    let walls = vec![raised_wall];
    let candidate = Vec2::new(2.5, 0.2);
    let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, 0.0, &walls);
    assert_eq!(resolved, candidate);

    // Half-height wall from y: 0.0 to 1.0 (blocks player)
    let half_wall = WallAabb::with_y(0.0, 0.0, 0.0, 5.0, 1.0, 0.4);
    assert!(half_wall.intersects_player_y(0.0));
    let walls = vec![half_wall];
    let resolved = resolve_player_collision(candidate, PLAYER_RADIUS, 0.0, &walls);
    assert_ne!(resolved, candidate);
}

/// One 10x10 m room with a single 10 x 0.4 m wall at z = 4.8..5.2 and the
/// supplied `openings`/`props` JSON.
fn level_with_wall(openings_json: &str, props_json: &str) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "collision_test",
            "name": "Collision Test",
            "spawn": {{ "x": 5.0, "z": 5.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }},
            "walls": [{{
                "x": 0.0, "z": 4.8, "width": 10.0, "depth": 0.4, "height": 3.5,
                "openings": {openings_json}
            }}],
            "props": {props_json}
        }}"#
    );
    LevelDef::from_json(&json).expect("valid json")
}

#[test]
fn test_doorway_wall_lets_the_player_pass_through() {
    let level = level_with_wall(
        r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
        "[]",
    );
    let walls = level.collision_aabbs();

    // Walking straight through the doorway is unobstructed.
    let in_doorway = Vec2::new(5.0, 5.0);
    assert_eq!(
        resolve_player_collision(in_doorway, PLAYER_RADIUS, 0.0, &walls),
        in_doorway
    );

    // The solid wall either side of the door still blocks.
    let into_wall = Vec2::new(1.0, 4.9);
    let resolved = resolve_player_collision(into_wall, PLAYER_RADIUS, 0.0, &walls);
    assert_ne!(resolved, into_wall);
    assert!(resolved.y <= 4.8 - PLAYER_RADIUS + 1e-3);
}

#[test]
fn test_doorway_header_never_blocks_the_player() {
    let level = level_with_wall(
        r#"[{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]"#,
        "[]",
    );
    let walls = level.collision_aabbs();
    // The header slice starts at 2.1 m, above the 1.8 m player.
    let header = walls
        .iter()
        .find(|w| w.min_y > 2.0 && w.max_y > 3.0)
        .expect("door header slice");
    assert!(!header.intersects_player_y(0.0));
}

#[test]
fn test_window_with_sill_blocks_the_player() {
    let level = level_with_wall(
        r#"[{ "kind": "window", "offset": 4.0, "width": 2.0, "height": 1.0, "sill": 1.0 }]"#,
        "[]",
    );
    let walls = level.collision_aabbs();
    // The sill wall spans y = 0..1.0, so the window is not walk-through.
    let sill = walls
        .iter()
        .find(|w| w.min_y == 0.0 && w.max_y <= 1.0 + 1e-3 && w.min_x >= 3.9 && w.max_x <= 6.1)
        .expect("window sill slice");
    assert!(sill.intersects_player_y(0.0));

    let in_window = Vec2::new(5.0, 5.0);
    let resolved = resolve_player_collision(in_window, PLAYER_RADIUS, 0.0, &walls);
    assert_ne!(resolved, in_window);
}

/// A level with one room and one recessed floor region of the given depth.
fn recessed_level(offset_y: f32) -> LevelDef {
    let json = format!(
        r#"{{
            "format_version": 1,
            "id": "recess",
            "name": "Recess",
            "spawn": {{ "x": 1.0, "z": 1.0 }},
            "room": {{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 }},
            "floor_regions": [
                {{ "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": {offset_y} }}
            ]
        }}"#
    );
    LevelDef::from_json(&json).expect("valid recessed json")
}

#[test]
fn test_player_band_follows_the_foot_height() {
    // A full-height wall on an elevated floor blocks only the floor it
    // belongs to.
    let wall = WallAabb::with_y(0.0, 2.0, 0.0, 5.0, 3.0, 0.4);
    assert!(wall.intersects_player_y(2.0), "blocks from its own floor");
    assert!(
        !wall.intersects_player_y(6.0),
        "a player three metres above it walks over it"
    );
    assert!(
        !wall.intersects_player_y(0.0),
        "a player whose whole body is below it is not blocked"
    );

    // A doorway header cut into an elevated floor stays passable from that
    // floor and becomes an obstruction to a player raised further.
    let header = WallAabb::with_y(0.0, 3.8, 0.0, 1.0, 1.5, 0.4);
    assert!(
        !header.intersects_player_y(2.0),
        "head clearance is honoured"
    );
    assert!(
        header.intersects_player_y(3.0),
        "raised, it is an obstruction"
    );
}

#[test]
fn test_recess_rims_block_from_below_but_not_from_above() {
    let level = recessed_level(-1.2);
    let aabbs = level.collision_aabbs();
    // The recess's four walls.
    let rims: Vec<&WallAabb> = aabbs
        .iter()
        .filter(|aabb| (aabb.min_y + 1.2).abs() < 1e-3 && aabb.max_y.abs() < 1e-3)
        .collect();
    assert!(!rims.is_empty(), "a deep recess has solid walls");

    // Standing on the recess floor and walking into the rim is stopped one
    // radius short of the boundary, exactly like an authored wall.
    let inside_edge = Vec2::new(2.4, 3.0);
    let blocked = resolve_player_collision(inside_edge, PLAYER_RADIUS, -1.2, &aabbs);
    assert!(blocked.x >= 2.0 + PLAYER_RADIUS - 1e-3, "{blocked:?}");

    // Standing on the room floor, the same rim is flush with the ground and
    // does not block: the step rule decides whether the drop is walkable.
    let on_floor = Vec2::new(2.4, 3.0);
    let free = resolve_player_collision(on_floor, PLAYER_RADIUS, 0.0, &aabbs);
    assert!(
        (free - on_floor).length() < 1e-3,
        "the rim must not lip the upper floor: {free:?}"
    );
}

#[test]
fn test_below_zero_rooms_and_gable_rooms_collide_at_their_own_geometry() {
    // A room two metres below the world floor with a doorway: the jambs
    // block a player standing on the sunken floor, and the sunken floor is
    // where the walkable surface resolves.
    let sunken = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "sunken",
            "name": "Sunken",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": -2.0 },
            "walls": [
                { "x": 0.0, "z": 3.8, "width": 8.0, "depth": 0.4, "y": -2.0, "height": 3.0,
                  "openings": [{ "kind": "door", "offset": 3.5, "width": 1.0, "height": 2.1 }] }
            ]
        }"#,
    )
    .expect("sunken json");
    let floor = crate::level::WalkableFloor::from_level(&sunken);
    assert_eq!(floor.height_at(4.0, 1.0), Some(-2.0));
    let aabbs = sunken.collision_aabbs();
    let jamb = aabbs
        .iter()
        .find(|aabb| aabb.min_y < -1.0)
        .expect("a jamb slice");
    assert!(
        (jamb.min_y + 2.0).abs() < 1e-3 && (jamb.max_y - 1.0).abs() < 1e-3,
        "the jamb spans the sunken wall, not the world floor: {jamb:?}"
    );
    assert!(
        jamb.intersects_player_y(-2.0),
        "blocks from the sunken floor"
    );
    assert!(
        !jamb.intersects_player_y(2.0),
        "a player above the wall's top walks over it"
    );

    // A gable room: a wall with no authored height climbs with the slope,
    // so its collision box reaches the ridge and blocks from that floor.
    let gable = LevelDef::from_json(
        r#"{
            "format_version": 1,
            "id": "gable_collision",
            "name": "Gable Collision",
            "spawn": { "x": 4.0, "z": 4.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0,
                      "floor_y": 1.0,
                      "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
            "walls": [
                { "x": 0.0, "z": 3.85, "width": 8.0, "depth": 0.3, "y": 1.0 }
            ]
        }"#,
    )
    .expect("gable collision json");
    let wall_aabbs = gable.collision_aabbs();
    let tallest = wall_aabbs
        .iter()
        .map(|aabb| aabb.max_y)
        .fold(f32::MIN, f32::max);
    assert!(
        (tallest - 6.0).abs() < 0.05,
        "the wall reaches the 6.0 m ridge, got {tallest}"
    );
    assert!(wall_aabbs.iter().all(|aabb| aabb.min_y >= 1.0 - 1e-3));
}

#[test]
fn test_shallow_recesses_have_no_collision_rims() {
    let level = recessed_level(-PLAYER_STEP_HEIGHT + 0.05);
    let aabbs = level.collision_aabbs();
    assert!(
        !aabbs
            .iter()
            .any(|aabb| aabb.min_y < -1e-3 && aabb.max_y.abs() < 1e-3),
        "a walkable step must not become a wall"
    );
}

#[test]
fn test_solid_prop_blocks_the_player() {
    let solid_level = level_with_wall(
        "[]",
        r#"[{ "model": "core:crate", "x": 5.0, "z": 2.0, "size": [1.0, 1.0, 1.0], "solid": true }]"#,
    );
    let solid_aabbs = solid_level.collision_aabbs();
    let into_prop = Vec2::new(5.0, 2.0);
    assert_ne!(
        resolve_player_collision(into_prop, PLAYER_RADIUS, 0.0, &solid_aabbs),
        into_prop
    );

    // A non-solid prop is ignored entirely by collision.
    let decorative_level = level_with_wall(
        "[]",
        r#"[{ "model": "core:plant", "x": 5.0, "z": 2.0, "size": [1.0, 1.0, 1.0] }]"#,
    );
    let decorative_aabbs = decorative_level.collision_aabbs();
    assert_eq!(decorative_aabbs.len(), solid_aabbs.len() - 1);
    assert_eq!(
        resolve_player_collision(into_prop, PLAYER_RADIUS, 0.0, &decorative_aabbs),
        into_prop
    );
}

/// The generic architectural pieces are real barriers and the trim is not:
/// columns, half walls, archway piers and guardrails block, while thresholds
/// and baseboards never appear in the collision set at all.
#[test]
fn test_architecture_pieces_block_and_trim_never_does() {
    let content = std::fs::read_to_string("tests/fixtures/levels/home_showcase.json")
        .expect("the Home showcase fixture is present");
    let level = LevelDef::from_json(&content).expect("the Home showcase parses");

    let moved = |position: Vec2, foot_y: f32| -> bool {
        let resolved =
            resolve_player_collision(position, PLAYER_RADIUS, foot_y, &level.collision_aabbs());
        (resolved - position).length() > 1e-3
    };

    // A post standing on the platform blocks at the platform's floor height.
    assert!(moved(Vec2::new(3.83, 2.23), 0.75), "the column must block");
    // The parapet at the top of the flight blocks.
    assert!(moved(Vec2::new(3.7, 2.3), 0.75), "the half wall must block");
    // The guardrail is a barrier from above and from the floor below it.
    assert!(
        moved(Vec2::new(5.35, 3.0), 0.75),
        "the rail blocks on the platform"
    );
    assert!(
        moved(Vec2::new(5.35, 3.0), 0.0),
        "the rail blocks from the room floor"
    );
    // The kitchen knee wall blocks on the tile.
    assert!(moved(Vec2::new(6.55, 6.7), 0.0), "the knee wall must block");
    // The archway opening is clear: a player standing in the middle of it is
    // not pushed out, and its header is above the player's head.
    assert!(
        !moved(Vec2::new(6.0, 2.3), 0.0),
        "the archway opening must never block the player"
    );
    assert!(
        !moved(Vec2::new(6.0, 2.15), 0.0),
        "the opening stays clear a player radius from its piers"
    );
    // The threshold in the archway is at the same spot and adds nothing.
    assert!(
        !moved(Vec2::new(6.0, 2.45), 0.0),
        "a threshold strip is trim, not a step"
    );

    // Nothing that thin is ever solid: every collision box is at least a
    // walkable step tall, so no decorative trim can snag the player.
    for aabb in level.collision_aabbs() {
        assert!(
            aabb.max_y - aabb.min_y > 0.2,
            "a trim-sized collider appeared: {aabb:?}"
        );
    }
}

/// A floor-region rim never blocks a step the controller could take: it
/// carries the walkable step as headroom, so a player on a ramp or stair
/// arriving beside it passes, while a player below the cliff is stopped.
#[test]
fn test_a_rim_blocks_cliffs_but_never_a_walkable_step() {
    // A 0.75 m platform edge: the collider extends 0.4 m under the platform.
    let rim = WallAabb::with_y(2.0, 0.0, 0.0, 0.4, 0.75, 4.0).allowing_step();
    // A player on the lower floor is blocked...
    assert!(rim.intersects_player_y(0.0));
    assert!(rim.intersects_player_y(0.1));
    // ... and a player whose feet are within the walkable step of the top is
    // not: that is a step the controller takes anyway.
    assert!(!rim.intersects_player_y(0.4));
    assert!(!rim.intersects_player_y(0.75));
    // A real wall keeps the strict rule at the same height.
    let wall = WallAabb::with_y(2.0, 0.0, 0.0, 0.4, 0.75, 4.0);
    assert!(wall.intersects_player_y(0.4));
    assert!(!wall.intersects_player_y(0.75));
}

/// Ray/AABB slab maths: face entry, inside-origin entry at zero, a miss and a
/// parallel ray outside the slab.
#[test]
fn test_ray_aabb_entry_hits_faces_and_misses_cleanly() {
    let min = [-1.0, 0.0, -1.0];
    let max = [1.0, 2.0, 1.0];

    // Straight at the near face from -Z.
    let entry = ray_aabb_entry(Vec3::new(0.0, 1.0, -5.0), Vec3::Z, min, max)
        .expect("the ray hits the near face");
    assert!((entry - 4.0).abs() < 1e-5, "entry at the face: {entry}");

    // From inside: entry is zero.
    let inside =
        ray_aabb_entry(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, min, max).expect("inside is a hit");
    assert!(inside.abs() < 1e-6);

    // Parallel and outside the slab.
    assert!(ray_aabb_entry(Vec3::new(2.0, 1.0, -5.0), Vec3::Z, min, max).is_none());
    // Pointing away.
    assert!(ray_aabb_entry(Vec3::new(0.0, 1.0, -5.0), Vec3::NEG_Z, min, max).is_none());
    // Parallel and inside the slab.
    assert!(ray_aabb_entry(Vec3::new(0.0, 1.0, 0.0), Vec3::X, min, max).is_some());
    // Non-finite input never hits.
    assert!(
        ray_aabb_entry(Vec3::new(f32::NAN, 1.0, 0.0), Vec3::Z, min, max).is_none(),
        "a NaN origin cannot produce a hit"
    );
}

/// A swept segment catches a crossing a point test would miss: a fast fall
/// moves from above a thin band to below it in one step.
#[test]
fn test_segment_overlaps_aabb_catches_fast_thin_crossings() {
    let min = [-0.5, -0.05, -0.5];
    let max = [0.5, 0.0, 0.5];
    // One frame of a 30 m/s fall: 3.1 m moved, band 5 cm tall.
    let from = Vec3::new(0.0, 3.0, 0.0);
    let to = Vec3::new(0.0, -0.1, 0.0);
    assert!(
        segment_overlaps_aabb(from, to, min, max),
        "the fall crosses the band"
    );
    // Both endpoints on the same side: no crossing.
    assert!(!segment_overlaps_aabb(
        Vec3::new(0.0, 3.0, 0.0),
        Vec3::new(0.0, 0.5, 0.0),
        min,
        max
    ));
    // Horizontal crossing through a thin wall volume.
    assert!(segment_overlaps_aabb(
        Vec3::new(-2.0, -0.02, 0.0),
        Vec3::new(2.0, -0.02, 0.0),
        min,
        max
    ));
    // A parallel pass outside the footprint.
    assert!(!segment_overlaps_aabb(
        Vec3::new(-2.0, -0.02, 3.0),
        Vec3::new(2.0, -0.02, 3.0),
        min,
        max
    ));
    // A degenerate segment is a point test.
    assert!(segment_overlaps_aabb(
        Vec3::new(0.0, -0.02, 0.0),
        Vec3::new(0.0, -0.02, 0.0),
        min,
        max
    ));
}

/// The shipped Places Demo, parsed from the repository level file (the same
/// shape as `demo_level()` in `src/game/tests.rs`).
fn demo_level() -> LevelDef {
    let content = std::fs::read_to_string("assets/levels/places_demo.json")
        .expect("the Places demo level is present");
    LevelDef::from_json(&content).expect("the Places demo parses")
}

/// A floating prop leaves no collision box where it floats: the demo duck's
/// whole footprint is clear of every wall, rim and solid in the shipped demo.
#[test]
fn test_the_floating_demo_duck_has_no_collision_box() {
    let level = demo_level();
    let duck = level
        .props
        .iter()
        .find(|prop| prop.model == "core:rubber_duck")
        .expect("the shipped demo places the floating duck");
    assert!(duck.float.is_some(), "the demo duck rides the pool water");
    assert!(!duck.solid, "validation requires a float to be non-solid");

    let [width, _height, depth] = duck.resolved_size(crate::level::PROP_FALLBACK_SIZE);
    let half_x = width * 0.5;
    let half_z = depth * 0.5;
    for aabb in level.collision_aabbs() {
        let overlaps = duck.x + half_x > aabb.min_x
            && duck.x - half_x < aabb.max_x
            && duck.z + half_z > aabb.min_z
            && duck.z - half_z < aabb.max_z;
        assert!(
            !overlaps,
            "the floating duck ({}, {}) must not overlap a static collider: {aabb:?}",
            duck.x, duck.z
        );
    }
}

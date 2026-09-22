//! Lighting-isolation regression cases.
//!
//! These tests are the acceptance suite for the wall-boundary repair: they use
//! the dedicated regression fixture `tests/fixtures/levels/lighting_isolation.json`,
//! whose thirteen plain cells each isolate one behaviour, and assert the rules
//! the baked lighting model must obey:
//!
//! ```text
//! opaque wall      blocks a direct fixture pool
//! opaque wall      blocks colour, not merely brightness
//! opaque wall      blocks darkness as well: a dark neighbour stays dark
//! doorway          transmits light through the hole it cuts
//! window           transmits light only over its sill and under its header
//! interior stub    blocks a pool inside one room
//! corner           has no artificial brightness collapse
//! ambient          a lightless room stays at the ambient floor
//! several lights   are evaluated one by one; a blocked light is not a veto
//! ```
//!
//! The level is a regression fixture, not packaged content; the file, not this
//! module, owns the layout.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::redundant_clone,
    clippy::suboptimal_flops,
    clippy::while_float
)]

use crate::level::LevelDef;
use crate::lighting::{AMBIENT_LEVEL, LevelLighting, LightColor};

/// Room indices in `tests/fixtures/levels/lighting_isolation.json`, in file order.
mod cell {
    pub const CORNER_WHITE: usize = 0;
    pub const DARK_NEIGHBOUR: usize = 1;
    pub const RED_SOURCE: usize = 2;
    pub const BLOCKED_FROM_RED: usize = 3;
    pub const DOOR_SOURCE: usize = 4;
    pub const THROUGH_DOOR: usize = 5;
    pub const WINDOW_SOURCE: usize = 6;
    pub const THROUGH_WINDOW: usize = 7;
    pub const RED_ROOM: usize = 8;
    pub const BLUE_ROOM: usize = 9;
    pub const AMBIENT_ONLY: usize = 10;
    pub const STUB_ROOM: usize = 11;
    pub const CORNER_RGB: usize = 12;
}

/// Loads and bakes the diagnostic level.
fn isolation() -> (LevelDef, LevelLighting) {
    let path = "tests/fixtures/levels/lighting_isolation.json";
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{path} must be readable: {error}"));
    let level =
        LevelDef::from_json(&content).unwrap_or_else(|error| panic!("{path} must parse: {error}"));
    let lighting = LevelLighting::bake(&level);
    assert_eq!(
        lighting.rooms().len(),
        13,
        "the diagnostic level is expected to keep its thirteen cells"
    );
    (level, lighting)
}

/// Baked illumination of one cell at a world position.
fn at(lighting: &LevelLighting, room: usize, x: f32, y: f32, z: f32) -> LightColor {
    lighting.sample_in_room(room, x, y, z)
}

/// True when every channel of `color` is within `tolerance` of ambient.
fn is_ambient(color: LightColor) -> bool {
    color
        .to_array()
        .iter()
        .all(|channel| (*channel - AMBIENT_LEVEL).abs() <= 1e-4)
}

#[test]
fn an_opaque_wall_blocks_a_white_fixture_pool() {
    let (_, lighting) = isolation();
    // The cell next door is empty, and its nearest point is 3.5 m from the
    // neighbouring cell's fixture — well inside the 6 m pool radius.
    for z in [0.5_f32, 3.0, 5.5] {
        let sample = at(&lighting, cell::DARK_NEIGHBOUR, 6.6, 1.5, z);
        assert!(
            is_ambient(sample),
            "white light crossed the opaque wall at z = {z}: {sample:?}"
        );
    }
    // Its wall face is equally dark, sampled the way the emitter samples it.
    let face_room = lighting.face_room(6.4, 3.0, -1.0, 0.0);
    let face = lighting.sample_face(
        face_room,
        6.4 - crate::lighting::WALL_FACE_PROBE_M,
        1.5,
        3.0,
    );
    assert!(
        is_ambient(face),
        "the dividing wall's far face must not be lit through itself: {face:?}"
    );
}

#[test]
fn an_opaque_wall_blocks_colour_not_only_brightness() {
    let (_, lighting) = isolation();
    // The fixture next door is pure red; the blocked cell must not pick up any
    // red beyond the ambient floor, in any channel.
    for z in [0.5_f32, 3.0, 5.5] {
        let sample = at(&lighting, cell::BLOCKED_FROM_RED, 19.4, 1.5, z);
        assert!(
            is_ambient(sample),
            "red light crossed the opaque wall at z = {z}: {sample:?}"
        );
    }
    // The blocked contribution is not a rounding error: distance alone would
    // have delivered a strong pool, so the ambient value above is the wall's
    // doing, not a fixture that was too far away to matter.
    let light = lighting
        .lights()
        .iter()
        .find(|light| (light.x - 15.8).abs() < 1e-3 && (light.z - 3.0).abs() < 1e-3)
        .expect("the red fixture is baked");
    let horizontal = ((19.4 - light.x).abs() - light.half_w).max(0.0);
    let vertical = 1.5 - light.y;
    let distance = horizontal.hypot(vertical);
    let unoccluded = crate::lighting::LOCAL_LIGHT_STRENGTH
        * light.intensity
        * light.height_factor
        * crate::lighting::smooth_falloff(distance / crate::lighting::LOCAL_LIGHT_RADIUS_M);
    assert!(
        unoccluded > 0.1,
        "the blocked pool must be large enough for the test to mean something: {unoccluded}"
    );
    // Sanity: the source cell itself is strongly red, so the fixture really
    // does emit what the blocked cell is being checked against.
    let source = at(&lighting, cell::RED_SOURCE, 15.8, 1.5, 3.0);
    assert!(
        source.r > source.b + 0.3,
        "the red cell must read red: {source:?}"
    );
}

#[test]
fn a_doorway_transmits_light_where_the_aperture_is() {
    let (_, lighting) = isolation();
    // The door spans z = 2.4..3.4 in the divider at x = 31.6..32.0.
    let source = at(&lighting, cell::DOOR_SOURCE, 28.6, 1.5, 3.0);
    let through_door = at(&lighting, cell::THROUGH_DOOR, 32.2, 1.5, 2.9);
    let far_side = at(&lighting, cell::THROUGH_DOOR, 37.8, 1.5, 2.9);
    assert!(
        source.luminance() > AMBIENT_LEVEL + 0.3,
        "the source cell must be lit for the doorway test to mean anything: {source:?}"
    );
    assert!(
        through_door.luminance() > AMBIENT_LEVEL + 0.02,
        "the doorway must transmit the neighbouring fixture: {through_door:?}"
    );
    assert!(
        through_door.luminance() > far_side.luminance() + 0.05,
        "the transmitted light must fall off away from the aperture: {through_door:?} vs {far_side:?}"
    );
    // Beside the door the same wall still blocks: the straight line from the
    // fixture to the sample leaves the aperture's own span.
    let beside = at(&lighting, cell::THROUGH_DOOR, 32.2, 1.5, 5.6);
    assert!(
        beside.luminance() < through_door.luminance() - 0.05,
        "the solid wall beside the door must keep blocking: {beside:?} vs {through_door:?}"
    );
}

#[test]
fn a_window_transmits_over_its_sill_but_not_under_it() {
    let (_, lighting) = isolation();
    // The window spans z = 2.4..3.4 and y = 1.0..2.2 in the divider at
    // x = 44.4..44.8.
    let source = at(&lighting, cell::WINDOW_SOURCE, 41.4, 1.5, 3.0);
    let over_sill = at(&lighting, cell::THROUGH_WINDOW, 45.0, 1.6, 2.9);
    let under_sill = at(&lighting, cell::THROUGH_WINDOW, 45.0, 0.05, 2.9);
    assert!(
        source.luminance() > AMBIENT_LEVEL + 0.3,
        "the source cell must be lit for the window test to mean anything: {source:?}"
    );
    assert!(
        over_sill.luminance() > AMBIENT_LEVEL + 0.02,
        "the window must transmit at its own height: {over_sill:?}"
    );
    assert!(
        is_ambient(under_sill),
        "the sill must block light below the opening: {under_sill:?}"
    );
}

#[test]
fn two_coloured_rooms_keep_their_own_light() {
    let (_, lighting) = isolation();
    // The red and blue cells share an opaque divider at x = 57.2..57.6. Each
    // keeps its own hue right up to the shared wall.
    let red_near_wall = at(&lighting, cell::RED_ROOM, 57.1, 1.5, 3.0);
    let blue_near_wall = at(&lighting, cell::BLUE_ROOM, 57.7, 1.5, 3.0);
    assert!(
        red_near_wall.r > red_near_wall.b + 0.2,
        "the red room must stay red at the shared wall: {red_near_wall:?}"
    );
    assert!(
        blue_near_wall.b > blue_near_wall.r + 0.2,
        "the blue room must stay blue at the shared wall: {blue_near_wall:?}"
    );
    // Neither neighbour contributes a trace of colour through the divider.
    assert!(
        red_near_wall.b <= AMBIENT_LEVEL + 1e-4,
        "blue light must not tint the red room: {red_near_wall:?}"
    );
    assert!(
        blue_near_wall.r <= AMBIENT_LEVEL + 1e-4,
        "red light must not tint the blue room: {blue_near_wall:?}"
    );
}

#[test]
fn a_dark_room_stays_dark_and_does_not_darken_its_neighbour() {
    let (_, lighting) = isolation();
    // The empty cell is a genuinely unlit room next to a lit one.
    for (x, z) in [(6.6_f32, 0.5_f32), (12.2, 3.0), (9.0, 5.5)] {
        let sample = at(&lighting, cell::DARK_NEIGHBOUR, x, 0.02, z);
        assert!(
            is_ambient(sample),
            "the unlit room must stay at ambient at ({x}, {z}): {sample:?}"
        );
    }
    // And the lit room keeps its own light: no darkness is pulled across the
    // wall into it.
    let lit = at(&lighting, cell::CORNER_WHITE, 5.8, 0.02, 3.0);
    assert!(
        lit.luminance() > AMBIENT_LEVEL + 0.3,
        "the lit room must not be darkened by its dark neighbour: {lit:?}"
    );
}

#[test]
fn an_interior_stub_blocks_a_pool_inside_one_room() {
    let (_, lighting) = isolation();
    // The stub spans z = 0..3 inside the 10 m cell, so a sample at z = 1.5 on
    // its far side cannot see the fixture. The stub stops short of the room's
    // z extent, so light can still wrap around its free end: the room stays a
    // single connected baseline area (partition-aware areas only split on walls
    // that cut the footprint in two) and the shadow shows up as the missing
    // pool, not as darkness below the baseline.
    let west_of_stub = at(&lighting, cell::STUB_ROOM, 73.0, 1.5, 1.5);
    let east_of_stub = at(&lighting, cell::STUB_ROOM, 77.0, 1.5, 1.5);
    let baseline = lighting.rooms()[cell::STUB_ROOM].baseline;
    // The fixture emits pure blue, so the blue channel is the one that moves.
    assert!(
        west_of_stub.b > baseline.b + 0.1,
        "the fixture must pool on its own side of the stub: {west_of_stub:?} vs {baseline:?}"
    );
    assert!(
        (east_of_stub.b - baseline.b).abs() < 0.02,
        "the stub must keep the pool out of its shadow: {east_of_stub:?} vs {baseline:?}"
    );
    // Around the stub's free end the light passes: the segment at z = 4 clears
    // the 3 m stub.
    let past_the_end = at(&lighting, cell::STUB_ROOM, 77.0, 1.5, 4.0);
    assert!(
        past_the_end.b > east_of_stub.b + 0.02,
        "light must pass around the stub's free end: {past_the_end:?} vs {east_of_stub:?}"
    );
}

#[test]
fn a_lit_corner_has_no_artificial_collapse() {
    let (_, lighting) = isolation();
    // The first cell's north and west walls straddle the room boundary and meet
    // at (0, 0); both faces are probed the way the geometry emitter probes them.
    let north_room = lighting.face_room(1.0, 0.15, 0.0, 1.0);
    let west_room = lighting.face_room(0.15, 1.0, 1.0, 0.0);
    assert_eq!(
        north_room, west_room,
        "both faces must resolve the same room"
    );
    let mut previous = (f32::NAN, 0.0_f32);
    for step in 0..=30 {
        let x = 0.2 + step as f32 * 0.1;
        let sample = lighting
            .sample_face(
                north_room,
                x,
                1.5,
                0.15 + crate::lighting::WALL_FACE_PROBE_M,
            )
            .luminance();
        assert!(sample.is_finite(), "corner sample at x = {x} is not finite");
        if previous.0.is_finite() {
            assert!(
                (sample - previous.1).abs() < 0.05,
                "the north face steps at x = {x}: {} -> {sample}",
                previous.1
            );
        }
        previous = (x, sample);
    }
    // The two faces agree at the join: no brightness cliff between them.
    let north_at_corner = lighting
        .sample_face(
            north_room,
            0.2,
            1.5,
            0.15 + crate::lighting::WALL_FACE_PROBE_M,
        )
        .luminance();
    let west_at_corner = lighting
        .sample_face(
            west_room,
            0.15 + crate::lighting::WALL_FACE_PROBE_M,
            1.5,
            0.2,
        )
        .luminance();
    assert!(
        (north_at_corner - west_at_corner).abs() < 0.05,
        "the faces meeting at the corner disagree: {north_at_corner} vs {west_at_corner}"
    );
    assert!(
        north_at_corner > AMBIENT_LEVEL + 0.3,
        "a lit corner must not collapse: {north_at_corner}"
    );
}

#[test]
fn every_fixture_contributes_independently_near_a_coloured_corner() {
    let (level, lighting) = isolation();
    let corner = at(&lighting, cell::CORNER_RGB, 81.3, 1.5, 1.5);
    assert!(
        corner.r > AMBIENT_LEVEL + 0.1,
        "the red fixture must reach the corner: {corner:?}"
    );
    assert!(
        corner.b > AMBIENT_LEVEL + 0.1,
        "the blue fixture must reach the corner: {corner:?}"
    );
    // Removing the red fixture must not disturb the blue contribution: each
    // light is evaluated on its own visibility, not as a group.
    let mut without_red = level.clone();
    without_red
        .ceiling_lights
        .retain(|light| light.emitted_color().r <= 0.5);
    let blue_only = LevelLighting::bake(&without_red);
    let corner_without_red = at(&blue_only, cell::CORNER_RGB, 81.3, 1.5, 1.5);
    assert!(
        (corner_without_red.b - corner.b).abs() < 1e-4,
        "blocking one fixture changed another: {corner_without_red:?} vs {corner:?}"
    );
    assert!(
        corner_without_red.r < corner.r,
        "the red fixture must be the source of the red channel"
    );
}

#[test]
fn an_unlit_room_is_exactly_ambient_everywhere() {
    let (_, lighting) = isolation();
    let room = &lighting.rooms()[cell::AMBIENT_ONLY];
    assert_eq!(room.effective_power, LightColor::BLACK);
    let mut z = room.z0 + 0.25;
    while z < room.z1 {
        let mut x = room.x0 + 0.25;
        while x < room.x1 {
            let sample = at(&lighting, cell::AMBIENT_ONLY, x, 1.5, z);
            assert!(
                is_ambient(sample),
                "an unlit room must stay at ambient at ({x}, {z}): {sample:?}"
            );
            x += 0.5;
        }
        z += 0.5;
    }
}

#[test]
fn emitted_wall_faces_are_lit_by_the_room_they_open_into() {
    // The emitter, not just the bake: a wall face that runs along a shared room
    // boundary must be shaded by the room it opens into. This is the artifact
    // the wall-boundary repair removes — a face lit by whichever neighbouring
    // room the containment tie-break preferred showed up as a dark, wrongly
    // coloured wedge in the corner — and it is asserted on emitted vertices so
    // a regression in the geometry emitter cannot slip past the bake tests.
    let (level, _) = isolation();
    let mesh = crate::render::build_level_geometry(&level);
    let walls = mesh.triangles_for(crate::render::SurfaceKind::Wall);
    assert!(!walls.is_empty(), "the diagnostic level must emit walls");

    // The red and blue cells share the divider at x = 57.2..57.6. Every wall
    // vertex on the red side of that plane must be red-dominant, and every one
    // on the blue side blue-dominant, with no vertex in between left at the
    // wrong room's hue.
    let mut red_vertices = 0;
    let mut blue_vertices = 0;
    for vertex in &walls {
        let [x, y, z] = vertex.pos;
        if !(0.4..=5.6).contains(&z) {
            continue;
        }
        if (57.0..=57.2).contains(&x) {
            red_vertices += 1;
            assert!(
                vertex.color[0] > vertex.color[2],
                "a red-side wall vertex at ({x}, {y}, {z}) is not red: {:?}",
                &vertex.color[..3]
            );
        } else if (57.6..=57.8).contains(&x) {
            blue_vertices += 1;
            assert!(
                vertex.color[2] > vertex.color[0],
                "a blue-side wall vertex at ({x}, {y}, {z}) is not blue: {:?}",
                &vertex.color[..3]
            );
        }
    }
    assert!(
        red_vertices > 0 && blue_vertices > 0,
        "the shared wall must emit vertices on both sides: {red_vertices}/{blue_vertices}"
    );

    // A wall face that runs along a shared room boundary must be lit by the
    // room it opens into, not by whichever neighbour the containment tie-break
    // preferred. `lighting_diagnostic`'s red cell (x 48..60) has a north wall
    // whose westmost sample sits exactly on the boundary with the blue cell:
    // the artifact was a blue-grey wedge at that end of a red room's wall. The
    // blue room's own wall ends at the same plane, so the check looks for the
    // red room's face among the vertices that share the position.
    let diag_path = "tests/fixtures/levels/lighting_diagnostic.json";
    let diagnostic = LevelDef::from_json(
        &std::fs::read_to_string(diag_path)
            .unwrap_or_else(|error| panic!("{diag_path} must be readable: {error}")),
    )
    .unwrap_or_else(|error| panic!("{diag_path} must parse: {error}"));
    let diag_mesh = crate::render::build_level_geometry(&diagnostic);
    let diag_walls: Vec<_> = diag_mesh
        .triangles_for(crate::render::SurfaceKind::Wall)
        .into_iter()
        .filter(|vertex| {
            let [x, _y, z] = vertex.pos;
            (47.9..=60.0).contains(&x) && z.abs() < 1e-3
        })
        .collect();
    let at_boundary = diag_walls
        .iter()
        .filter(|vertex| vertex.pos[0] <= 48.1)
        .count();
    let red_at_boundary = diag_walls
        .iter()
        .filter(|vertex| vertex.pos[0] <= 48.1 && vertex.color[0] > vertex.color[2])
        .count();
    assert!(
        at_boundary > 0,
        "the boundary end of the red cell's north wall must emit vertices"
    );
    assert!(
        red_at_boundary > 0,
        "the red cell's wall sample on the shared boundary is not red at all: \
         {red_at_boundary}/{at_boundary} red vertices"
    );
    // Inside the red cell's own span the whole wall face must be red-dominant.
    for vertex in diag_walls.iter().filter(|vertex| vertex.pos[0] >= 50.0) {
        assert!(
            vertex.color[0] > vertex.color[2],
            "a red room's wall vertex at {:?} carries the blue neighbour's light: {:?}",
            vertex.pos,
            &vertex.color[..3]
        );
    }
}

#[test]
fn isolation_geometry_bakes_safe_vertices() {
    let (level, _) = isolation();
    let mesh = crate::render::build_level_geometry(&level);
    crate::lighting_audit::assert_vertex_colors_safe(&mesh.all_vertices());
    assert!(
        mesh.index_count_for_family(crate::render::SurfaceKind::Wall) > 0,
        "the diagnostic level must emit walls"
    );
}

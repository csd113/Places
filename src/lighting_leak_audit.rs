//! End-to-end acceptance checks for the shipped demo's static lighting.
//!
//! The unit tests around `crate::lighting` verify the model's pieces against
//! hand-built rooms. These checks go the other way: they build the *shipped*
//! level through the same public API the game uses and compare the baked
//! fixture pools against an independent, exact visibility reference built from
//! the same wall solid geometry.
//!
//! The reference is deliberately naive — one unshrunk axis-aligned box per
//! solid wall patch, one straight segment per fixture with a millimetre of
//! start displacement — so a leak caused by the production code's optimisations
//! (per-site prefiltering, panel-edge sources, clamping) shows up as a
//! difference rather than being reproduced by a mirror of the same bug.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic in tests.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use crate::level::{LevelDef, LevelSurfaces, WallAxis, wall_solid_slices_profiled};
use crate::lighting::{LevelLighting, smooth_falloff};

/// One exact wall solid, as a world-space axis-aligned box (no shrink).
#[derive(Clone, Copy, Debug)]
struct Solid {
    min: [f32; 3],
    max: [f32; 3],
}

/// Every solid patch of every authored wall, exactly as the mesh emits it.
fn wall_solids(level: &LevelDef) -> Vec<Solid> {
    let surfaces = LevelSurfaces::new(level);
    let mut out = Vec::new();
    for wall in &level.walls {
        let breaks = surfaces.wall_profile_breaks(wall);
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &breaks,
        );
        let axis = wall.axis();
        let (origin_x, origin_z) = wall.length_origin();
        let (x0, x1) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (z0, z1) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        for slice in slices {
            let (l_min, l_max) = match axis {
                WallAxis::X => (origin_x + slice.start, origin_x + slice.end),
                WallAxis::Z => (origin_z + slice.start, origin_z + slice.end),
            };
            let (a_min, a_max) = match axis {
                WallAxis::X => (z0, z1),
                WallAxis::Z => (x0, x1),
            };
            let (min, max) = match axis {
                WallAxis::X => ([l_min, slice.bottom, a_min], [l_max, slice.top, a_max]),
                WallAxis::Z => ([a_min, slice.bottom, l_min], [a_max, slice.top, l_max]),
            };
            out.push(Solid { min, max });
        }
    }
    out
}

/// One zero-thickness floor interface, as the bake builds it: a stair-step of
/// horizontal planes, one per floor-grid cell at that cell's own height.
#[derive(Clone, Copy, Debug)]
struct FloorInterface {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    y: f32,
}

/// Every floor interface of every room, from the same floor grid collision uses.
fn floor_interfaces(level: &LevelDef) -> Vec<FloorInterface> {
    let surfaces = LevelSurfaces::new(level);
    let mut out = Vec::new();
    for room in surfaces.rooms() {
        let grid = surfaces.floor_grid(room);
        for (iz, z_span) in grid.zs.windows(2).enumerate() {
            let &[z0, z1] = z_span else {
                continue;
            };
            for (ix, x_span) in grid.xs.windows(2).enumerate() {
                let &[x0, x1] = x_span else {
                    continue;
                };
                out.push(FloorInterface {
                    x0,
                    x1,
                    z0,
                    z1,
                    y: grid.y_at(room, ix, iz),
                });
            }
        }
    }
    out
}

/// True when the segment crosses the interface inside its footprint. An
/// endpoint exactly on the plane does not count, mirroring the bake.
fn segment_crosses_interface(interface: FloorInterface, from: [f32; 3], to: [f32; 3]) -> bool {
    let from_side = from[1] - interface.y;
    let to_side = to[1] - interface.y;
    if from_side * to_side >= 0.0 {
        return false;
    }
    let denominator = from_side - to_side;
    if denominator == 0.0 {
        return false;
    }
    let t = from_side / denominator;
    if !(0.0..=1.0).contains(&t) {
        return false;
    }
    let x = (to[0] - from[0]).mul_add(t, from[0]);
    let z = (to[2] - from[2]).mul_add(t, from[2]);
    x >= interface.x0 && x <= interface.x1 && z >= interface.z0 && z <= interface.z1
}

/// True when the segment `from`-`to` crosses the solid.
fn segment_hits_solid(solid: Solid, from: [f32; 3], to: [f32; 3]) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for axis in 0..3 {
        let (start, end) = (from[axis], to[axis]);
        let delta = end - start;
        if delta.abs() <= f32::EPSILON {
            if start < solid.min[axis] || start > solid.max[axis] {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / delta;
        let mut near = (solid.min[axis] - start) * inverse;
        let mut far = (solid.max[axis] - start) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return false;
        }
    }
    true
}

/// The exact local pool at a point: the smooth falloff of every fixture whose
/// panel point has an unblocked segment to the point, capped like the bake.
fn reference_pool(
    lighting: &LevelLighting,
    solids: &[Solid],
    interfaces: &[FloorInterface],
    x: f32,
    y: f32,
    z: f32,
) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    for light in lighting.lights() {
        let dx = ((x - light.x()).abs() - light.half_w()).max(0.0);
        let dz = ((z - light.z()).abs() - light.half_d()).max(0.0);
        let horizontal_squared = dx * dx + dz * dz;
        let vertical = y - light.y();
        let distance_squared = vertical.mul_add(vertical, horizontal_squared);
        let radius_squared = 6.0 * 6.0;
        if !distance_squared.is_finite() || distance_squared >= radius_squared {
            continue;
        }
        let source = [
            x.clamp(light.x() - light.half_w(), light.x() + light.half_w()),
            light.y(),
            z.clamp(light.z() - light.half_d(), light.z() + light.half_d()),
        ];
        // Start a millimetre along the segment so a fixture mounted flush with
        // a face is not blocked by the wall it sits on, exactly like the
        // production query.
        let delta = [x - source[0], y - source[1], z - source[2]];
        let length = delta[0]
            .mul_add(delta[0], delta[1].mul_add(delta[1], delta[2] * delta[2]))
            .sqrt();
        let start = if length > 0.0 {
            [
                (delta[0] / length).mul_add(1e-3, source[0]),
                (delta[1] / length).mul_add(1e-3, source[1]),
                (delta[2] / length).mul_add(1e-3, source[2]),
            ]
        } else {
            source
        };
        if solids
            .iter()
            .any(|solid| segment_hits_solid(*solid, start, [x, y, z]))
            || interfaces
                .iter()
                .any(|interface| segment_crosses_interface(*interface, start, [x, y, z]))
        {
            continue;
        }
        let falloff = smooth_falloff(distance_squared.sqrt() / 6.0);
        let strength = 0.42 * light.intensity() * light.height_factor * falloff;
        sum[0] += strength * light.color().r;
        sum[1] += strength * light.color().g;
        sum[2] += strength * light.color().b;
    }
    [
        sum[0].clamp(0.0, 0.45),
        sum[1].clamp(0.0, 0.45),
        sum[2].clamp(0.0, 0.45),
    ]
}

/// Moves a sample out of a wall the same way the bake's `clear_sample` does, so
/// both models are evaluated at the same position.
fn cleared(level: &LevelSurfaces<'_>, solids: &[Solid], room: usize, x: f32, z: f32) -> (f32, f32) {
    let inside = |x: f32, z: f32| {
        solids.iter().any(|solid| {
            x >= solid.min[0] && x <= solid.max[0] && z >= solid.min[2] && z <= solid.max[2]
        })
    };
    if !inside(x, z) {
        return (x, z);
    }
    let Some(room) = level.rooms().get(room) else {
        return (x, z);
    };
    let (rx0, rx1, rz0, rz1) = room.bounds();
    let (cx, cz) = (f32::midpoint(rx0, rx1), f32::midpoint(rz0, rz1));
    let (dx, dz) = (cx - x, cz - z);
    let distance = dx.hypot(dz);
    if !distance.is_finite() || distance <= 1e-3 {
        return (x, z);
    }
    for step in 1..=64u16 {
        let walked = f32::from(step) * 0.05;
        if walked > distance {
            break;
        }
        let t = walked / distance;
        let probe = (dx.mul_add(t, x), dz.mul_add(t, z));
        if !inside(probe.0, probe.1) {
            return probe;
        }
    }
    (cx, cz)
}

fn shipped_demo() -> LevelDef {
    LevelDef::from_json(include_str!("../assets/levels/places_demo.json"))
        .expect("the shipped places_demo parses")
}

/// Compares one baked sample against the exact-visibility reference, on every
/// channel, after removing the room's own baseline and its legitimate doorway
/// blend.
fn assert_pool_matches_reference(
    lighting: &LevelLighting,
    solids: &[Solid],
    interfaces: &[FloorInterface],
    room: usize,
    point: [f32; 3],
    context: &str,
) {
    let [x, y, z] = point;
    let baked = lighting.sample_in_room(room, x, y, z);
    // The sample's own connected area, not the room-wide average: a
    // partitioned room must be audited against the baseline it actually got.
    let baseline = lighting.baseline_in_room(room, x, z);
    let blend = lighting.opening_blend(room, x, y, z);
    let reference = reference_pool(lighting, solids, interfaces, x, y, z);
    for (channel, (baked, reference)) in [
        (baked.r - baseline.r - blend.r, reference[0]),
        (baked.g - baseline.g - blend.g, reference[1]),
        (baked.b - baseline.b - blend.b, reference[2]),
    ]
    .into_iter()
    .enumerate()
    {
        let excess = baked - reference;
        assert!(
            excess <= 0.02,
            "{context} at ({x:.2}, {y:.2}, {z:.2}) channel {channel}: \
             baked pool {baked:.4} exceeds the exact reference {reference:.4} by {excess:.4}"
        );
    }
}

#[test]
fn the_shipped_demos_fixture_pools_are_occlusion_exact() {
    let level = shipped_demo();
    let lighting = LevelLighting::bake(&level);
    let solids = wall_solids(&level);
    let interfaces = floor_interfaces(&level);
    let surfaces = LevelSurfaces::new(&level);
    assert!(
        !solids.is_empty() && !lighting.lights().is_empty(),
        "the demo must have walls and fixtures to check"
    );

    // Every floor grid sample. A leak through a wall, a jamb or a corner shows
    // up here as extra pool light.
    for (room_index, room) in surfaces.rooms().iter().enumerate() {
        let grid = surfaces.floor_grid(room);
        for z in &grid.zs {
            for x in &grid.xs {
                let y = surfaces.floor_y_at(*x, *z).unwrap_or(room.floor_y);
                let (px, pz) = cleared(&surfaces, &solids, room_index, *x, *z);
                assert_pool_matches_reference(
                    &lighting,
                    &solids,
                    &interfaces,
                    room_index,
                    [px, y, pz],
                    &format!("floor of room {room_index}"),
                );
            }
        }
    }

    // Wall faces too: a face is sampled 25 cm into the room it opens into, and
    // a seam leak shows up as extra light at the opening's own height. The
    // wall's top edge is where the seam between a solid column and an opening
    // column runs unbroken, so it is checked explicitly.
    let mut checked = 0usize;
    for wall in &level.walls {
        let axis = wall.axis();
        let (x0, x1) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (z0, z1) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        let breaks = surfaces.wall_profile_breaks(wall);
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &breaks,
        );
        let (origin_x, origin_z) = wall.length_origin();
        for slice in &slices {
            if slice.end - slice.start <= 0.2 {
                continue;
            }
            for step in 0..=6 {
                let t = step as f32 / 6.0;
                let along = (slice.end - slice.start).mul_add(t, slice.start);
                for y in [
                    slice.bottom + 0.05,
                    f32::midpoint(slice.bottom, slice.top),
                    slice.top - 0.05,
                ] {
                    for (position, normal) in match axis {
                        WallAxis::X => [(z0, [0.0f32, 0.0, -1.0]), (z1, [0.0, 0.0, 1.0])],
                        WallAxis::Z => [(x0, [-1.0f32, 0.0, 0.0]), (x1, [1.0, 0.0, 0.0])],
                    } {
                        let probe = match axis {
                            WallAxis::X => [origin_x + along, y, normal[2].mul_add(0.25, position)],
                            WallAxis::Z => [normal[0].mul_add(0.25, position), y, origin_z + along],
                        };
                        let Some(room) = lighting
                            .room_index_strict_at(probe[0], probe[2])
                            .or_else(|| lighting.room_index_at(probe[0], probe[2]))
                        else {
                            continue;
                        };
                        // A probe beside a junction can sit inside another
                        // wall's footprint; the bake walks such a sample out
                        // first, so the reference must use the same position.
                        let (px, pz) = cleared(&surfaces, &solids, room, probe[0], probe[2]);
                        assert_pool_matches_reference(
                            &lighting,
                            &solids,
                            &interfaces,
                            room,
                            [px, probe[1], pz],
                            "wall face",
                        );
                        checked = checked.saturating_add(1);
                    }
                }
            }
        }
    }
    assert!(checked > 200, "the demo must expose wall faces to check");
}

#[test]
fn the_shipped_demos_wall_faces_open_into_their_own_room() {
    // A wall face is lit by the room it looks into. Sampling each length face
    // 25 cm out and comparing against that room's own query catches a face
    // that sampled its neighbour's baseline instead — the coloured wedge that
    // reads as light bleeding around a corner.
    let level = shipped_demo();
    let lighting = LevelLighting::bake(&level);
    let surfaces = LevelSurfaces::new(&level);
    let mut checked = 0usize;
    for wall in &level.walls {
        let axis = wall.axis();
        let (x0, x1) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (z0, z1) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        let breaks = surfaces.wall_profile_breaks(wall);
        let slices = wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &breaks,
        );
        let (origin_x, origin_z) = wall.length_origin();
        for slice in &slices {
            if slice.end - slice.start <= 0.2 {
                continue;
            }
            let along = f32::midpoint(slice.start, slice.end);
            let y = f32::midpoint(slice.bottom, slice.top);
            for (position, normal) in match axis {
                WallAxis::X => [(z0, [0.0f32, 0.0, -1.0]), (z1, [0.0, 0.0, 1.0])],
                WallAxis::Z => [(x0, [-1.0f32, 0.0, 0.0]), (x1, [1.0, 0.0, 0.0])],
            } {
                let point = match axis {
                    WallAxis::X => [origin_x + along, y, position],
                    WallAxis::Z => [position, y, origin_z + along],
                };
                let probe = [
                    normal[0].mul_add(0.25, point[0]),
                    point[1],
                    normal[2].mul_add(0.25, point[2]),
                ];
                let Some(room) = lighting
                    .room_index_strict_at(probe[0], probe[2])
                    .or_else(|| lighting.room_index_at(probe[0], probe[2]))
                else {
                    continue;
                };
                let used = lighting.sample_face(
                    lighting.face_room(point[0], point[2], normal[0], normal[2]),
                    probe[0],
                    probe[1],
                    probe[2],
                );
                let expected = lighting.sample_in_room(room, probe[0], probe[1], probe[2]);
                for (used, expected) in [
                    (used.r, expected.r),
                    (used.g, expected.g),
                    (used.b, expected.b),
                ] {
                    assert!(
                        (used - expected).abs() <= 0.05,
                        "a wall face at ({:.2}, {:.2}, {:.2}) is not lit by room {room}: \
                         {used:.3} vs {expected:.3}",
                        probe[0],
                        probe[1],
                        probe[2]
                    );
                }
                checked = checked.saturating_add(1);
            }
        }
    }
    assert!(checked > 20, "the demo must expose wall faces to check");
}

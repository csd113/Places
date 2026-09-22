//! Surface-coincidence regression tests for the level geometry builder.
//!
//! The builder may emit two coplanar triangles only when the level genuinely
//! asks for it (an intentionally overlapping prop, two rooms a creator placed
//! on top of each other). It must never emit a *coincident* pair as a
//! by-product of how it derives walls, openings, thresholds or groups: two
//! surfaces at the same depth fight for the same pixels, and the loser flickers
//! as the camera moves.
//!
//! These tests build real meshes through the same entry point the game uses and
//! measure the emitted triangles, rather than asserting that a level parses.
//! The `places_demo` case is the shipped acceptance check: it must emit no
//! coincident static surface at all.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]

use std::collections::HashMap;

use crate::level::LevelDef;
use crate::loader::PropCatalog;
use crate::materials::MaterialTable;
use crate::render::{
    DECAL_POLYGON_OFFSET, LevelMesh, MaterialIndex, SurfaceKind,
    build_level_geometry_with_catalog_and_materials,
};

/// One emitted triangle, reduced to its plane and its 3D corners.
#[derive(Clone, Copy, Debug)]
struct Triangle {
    points: [[f32; 3]; 3],
    kind: SurfaceKind,
    material: MaterialIndex,
    normal: [f32; 3],
    /// Plane offset `n . p`.
    offset: f32,
}

/// Every triangle of a mesh, with its plane resolved and normalised.
fn triangles(mesh: &LevelMesh) -> Vec<Triangle> {
    let mut out = Vec::new();
    for range in &mesh.ranges {
        for chunk in range.indices.chunks(3) {
            let mut points = [[0.0f32; 3]; 3];
            let mut complete = true;
            for (slot, index) in chunk.iter().enumerate() {
                let Some(vertex) = range.vertices.get(usize::from(*index)) else {
                    complete = false;
                    break;
                };
                points[slot] = vertex.pos;
            }
            if !complete {
                continue;
            }
            let edge0 = sub(points[1], points[0]);
            let edge1 = sub(points[2], points[0]);
            let cross = cross(edge0, edge1);
            let length = dot(cross, cross).sqrt();
            if !length.is_finite() || length < 1e-12 {
                continue;
            }
            let normal = cross.map(|v| v / length);
            out.push(Triangle {
                points,
                kind: range.key.kind,
                material: range.key.material,
                normal,
                offset: dot(normal, points[0]),
            });
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A pair of triangles that lie in the same plane and overlap.
#[derive(Debug)]
struct Overlap {
    first: usize,
    second: usize,
    area: f32,
}

impl Overlap {
    fn describe(&self, all: &[Triangle]) -> String {
        let a = all[self.first];
        let b = all[self.second];
        format!(
            "{:?} (material {}, normal {:?}, at {:?}) overlaps {:?} (material {}, normal {:?}, at {:?}) by {:.4} m^2",
            a.kind,
            a.material,
            a.normal,
            a.points[0],
            b.kind,
            b.material,
            b.normal,
            b.points[0],
            self.area
        )
    }
}

/// Every pair of triangles that share a plane (within `plane_tolerance`) and
/// overlap in that plane by more than `area_tolerance`.
///
/// The triangles are bucketed by a quantised plane key, which is exact for the
/// axis-aligned surfaces the builder emits; each bucket is then checked
/// pairwise with a real 2D polygon intersection, so touching neighbours are
/// never reported and a genuine overlap of any size is.
fn coincident_overlaps(
    all: &[Triangle],
    plane_tolerance: f32,
    area_tolerance: f32,
) -> Vec<Overlap> {
    let mut buckets: HashMap<(i32, i32, i32, i32), Vec<usize>> = HashMap::new();
    for (index, triangle) in all.iter().enumerate() {
        let key = (
            (triangle.normal[0] * 256.0).round() as i32,
            (triangle.normal[1] * 256.0).round() as i32,
            (triangle.normal[2] * 256.0).round() as i32,
            (triangle.offset * 256.0).round() as i32,
        );
        buckets.entry(key).or_default().push(index);
    }
    let mut overlaps = Vec::new();
    for members in buckets.values() {
        for a in 0..members.len() {
            for b in (a + 1)..members.len() {
                let (first, second) = (members[a], members[b]);
                let (Some(ta), Some(tb)) = (all.get(first), all.get(second)) else {
                    continue;
                };
                // A pair that faces opposite ways is a wall with its own back
                // face, not two surfaces competing for a pixel.
                if dot(ta.normal, tb.normal) <= 0.0 {
                    continue;
                }
                if (ta.offset - tb.offset).abs() > plane_tolerance {
                    continue;
                }
                let area = overlap_area(ta, tb);
                if area > area_tolerance {
                    overlaps.push(Overlap {
                        first,
                        second,
                        area,
                    });
                }
            }
        }
    }
    overlaps
}

/// The area of the intersection of two coplanar triangles, projected onto the
/// plane's own 2D basis.
fn overlap_area(a: &Triangle, b: &Triangle) -> f32 {
    let normal = glam::Vec3::from(a.normal);
    let tangent = if normal.dot(glam::Vec3::Y).abs() > 0.9 {
        glam::Vec3::X
    } else {
        glam::Vec3::Y.cross(normal).normalize()
    };
    let bitangent = normal.cross(tangent);
    let to_2d = |p: [f32; 3]| {
        let v = glam::Vec3::from(p);
        [v.dot(tangent), v.dot(bitangent)]
    };
    let mut subject: Vec<[f32; 2]> = a.points.iter().map(|p| to_2d(*p)).collect();
    let clip: Vec<[f32; 2]> = b.points.iter().map(|p| to_2d(*p)).collect();
    for edge in 0..3 {
        let start = clip[edge];
        let end = clip[(edge + 1) % 3];
        subject = clip_polygon(&subject, start, end);
        if subject.is_empty() {
            return 0.0;
        }
    }
    polygon_area(&subject)
}

/// Clips a convex polygon against the half-plane left of `start -> end`.
fn clip_polygon(subject: &[[f32; 2]], start: [f32; 2], end: [f32; 2]) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(subject.len() + 1);
    let inside = |p: [f32; 2]| {
        (end[0] - start[0]) * (p[1] - start[1]) - (end[1] - start[1]) * (p[0] - start[0]) >= -1e-9
    };
    for (index, &current) in subject.iter().enumerate() {
        let previous = subject[(index + subject.len() - 1) % subject.len()];
        let current_inside = inside(current);
        let previous_inside = inside(previous);
        if current_inside {
            if !previous_inside
                && let Some(point) = line_intersection(previous, current, start, end)
            {
                out.push(point);
            }
            out.push(current);
        } else if previous_inside
            && let Some(point) = line_intersection(previous, current, start, end)
        {
            out.push(point);
        }
    }
    out
}

fn line_intersection(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> Option<[f32; 2]> {
    let d0 = [a1[0] - a0[0], a1[1] - a0[1]];
    let d1 = [b1[0] - b0[0], b1[1] - b0[1]];
    let denominator = d0[0] * d1[1] - d0[1] * d1[0];
    if denominator.abs() < 1e-12 {
        return None;
    }
    let t = ((b0[0] - a0[0]) * d1[1] - (b0[1] - a0[1]) * d1[0]) / denominator;
    Some([a0[0] + d0[0] * t, a0[1] + d0[1] * t])
}

fn polygon_area(points: &[[f32; 2]]) -> f32 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        area += current[0] * next[1] - next[0] * current[1];
    }
    (area * 0.5).abs()
}

/// Builds a level through the shipped asset pipeline (so decals resolve too).
fn shipped_mesh(level: &LevelDef) -> LevelMesh {
    let catalog = PropCatalog::load_default();
    let materials = MaterialTable::logical(level, catalog.assets(), None);
    build_level_geometry_with_catalog_and_materials(level, &catalog, &materials)
}

fn parse(json: &str) -> LevelDef {
    LevelDef::from_json(json).expect("test level parses")
}

/// Fails with every offending pair listed when any coincident overlap exists.
fn assert_no_coincident_overlaps(all: &[Triangle], context: &str) {
    let overlaps = coincident_overlaps(all, 1e-4, 1e-4);
    assert!(
        overlaps.is_empty(),
        "{context}: {} coincident overlapping triangle pair(s):\n{}",
        overlaps.len(),
        overlaps
            .iter()
            .take(8)
            .map(|overlap| overlap.describe(all))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Two 4 m deep rooms meeting at x = 4.0, separated by a 0.4 m wall centred on
/// that shared boundary — the construction the shipped demo uses, so the two
/// floors joint cover the wall's footprint and the doorway threshold is a real
/// walkable surface.
fn adjacent_rooms(openings_json: &str, floors: &str, wall_material: &str) -> LevelDef {
    let floor_line = if floors.is_empty() {
        String::new()
    } else {
        format!(", {floors}")
    };
    let material_line = if wall_material.is_empty() {
        String::new()
    } else {
        format!(", \"material\": \"{wall_material}\"")
    };
    parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "adjacent_rooms",
            "name": "Adjacent Rooms",
            "spawn": {{ "x": 2.0, "z": 2.0 }},
            "rooms": [
                {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0{floor_line} }},
                {{ "x": 4.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0{floor_line} }}
            ],
            "walls": [
                {{ "x": 3.8, "z": 0.0, "width": 0.4, "depth": 4.0, "height": 3.0{material_line},
                   "openings": {openings_json} }}
            ]
        }}"#
    ))
}

#[test]
fn a_doorway_threshold_is_covered_by_exactly_one_floor_surface() {
    let level = adjacent_rooms(
        r#"[{ "kind": "door", "offset": 1.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#,
        "",
        "",
    );
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);

    // Nothing in the whole level may fight for a pixel: the two rooms' floors
    // meet at the shared boundary and the doorway is a hole through the wall,
    // never a second floor quad.
    assert_no_coincident_overlaps(&all, "two connected rooms with a doorway");

    // And the threshold really is covered: every point of the shared plane
    // inside the doorway is on exactly one floor surface, so the player walks
    // on geometry the renderer actually draws.
    let floor_triangles: Vec<&Triangle> = all
        .iter()
        .filter(|triangle| triangle.kind == SurfaceKind::Floor)
        .collect();
    assert!(!floor_triangles.is_empty());
    for x in [3.98f32, 3.995, 4.005, 4.02] {
        for z in [1.6f32, 2.0, 2.4] {
            let covering = floor_triangles
                .iter()
                .filter(|triangle| covers_xz(triangle, x, z))
                .count();
            assert_eq!(
                covering, 1,
                "floor coverage at ({x}, {z}) is {covering}, not 1"
            );
        }
    }
}

/// True when `(x, z)` lies inside the triangle's XZ projection.
fn covers_xz(triangle: &Triangle, x: f32, z: f32) -> bool {
    let first = triangle.points[0];
    let second = triangle.points[1];
    let third = triangle.points[2];
    let edge = |p: [f32; 3], q: [f32; 3]| (q[0] - p[0]) * (z - p[2]) - (q[2] - p[2]) * (x - p[0]);
    let (e0, e1, e2) = (edge(first, second), edge(second, third), edge(third, first));
    let has_negative = e0 < -1e-6 || e1 < -1e-6 || e2 < -1e-6;
    let has_positive = e0 > 1e-6 || e1 > 1e-6 || e2 > 1e-6;
    !(has_negative && has_positive)
}

#[test]
fn adjacent_rooms_with_different_floor_materials_do_not_overlap() {
    let floors = r#""material": "core:carpet_damp_01""#;
    let level = parse(&format!(
        r#"{{
            "format_version": 1,
            "id": "floor_materials",
            "name": "Floor Materials",
            "spawn": {{ "x": 2.0, "z": 2.0 }},
            "rooms": [
                {{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }},
                {{ "x": 4.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0, {floors} }}
            ],
            "walls": [
                {{ "x": 3.8, "z": 0.0, "width": 0.4, "depth": 4.0, "height": 3.0,
                   "openings": [{{ "kind": "door", "offset": 1.5, "width": 1.0, "height": 2.1, "sill": 0.0 }}] }}
            ]
        }}"#
    ));
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    assert_no_coincident_overlaps(&all, "rooms with different floor materials");

    // Each room's own floor material stays on its own side of the wall.
    let beige = MaterialTable::logical(&level, PropCatalog::load_default().assets(), None)
        .index_of("core:carpet_beige_01");
    let damp = MaterialTable::logical(&level, PropCatalog::load_default().assets(), None)
        .index_of("core:carpet_damp_01");
    let (Some(beige), Some(damp)) = (beige, damp) else {
        panic!("the demo's floor materials must resolve");
    };
    for triangle in &all {
        if triangle.kind != SurfaceKind::Floor {
            continue;
        }
        let mid_x = f32::midpoint(
            triangle.points[0][0]
                .min(triangle.points[1][0])
                .min(triangle.points[2][0]),
            triangle.points[0][0]
                .max(triangle.points[1][0])
                .max(triangle.points[2][0]),
        );
        if triangle.material == beige {
            assert!(mid_x < 4.0 + 1e-3, "beige floor leaked past the divider");
        }
        if triangle.material == damp {
            assert!(mid_x > 4.0 - 1e-3, "damp floor leaked past the divider");
        }
    }
}

#[test]
fn multiple_openings_on_one_wall_keep_one_surface_per_span() {
    let level = adjacent_rooms(
        r#"[
            { "kind": "door", "offset": 0.4, "width": 0.9, "height": 2.1, "sill": 0.0 },
            { "kind": "window", "offset": 1.6, "width": 0.8, "height": 1.0, "sill": 1.4 },
            { "kind": "window", "offset": 2.7, "width": 0.8, "height": 1.0, "sill": 1.4 }
        ]"#,
        "",
        "",
    );
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    assert_no_coincident_overlaps(&all, "three openings on one wall");
}

#[test]
fn coincident_walls_with_different_vertical_extents_emit_one_surface() {
    // The next room's wall continues the same plane at a different base and
    // height: the shared span must be emitted once, not twice.
    let json = r#"{
        "format_version": 1,
        "id": "continued_wall",
        "name": "Continued Wall",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 },
            { "x": 0.0, "z": 4.4, "width": 4.0, "depth": 4.0, "height": 3.0,
              "floor_y": -1.0, "material": "core:carpet_damp_01" }
        ],
        "walls": [
            { "x": 0.0, "z": 0.2, "width": 0.3, "depth": 3.7, "height": 3.0 },
            { "x": 0.0, "z": 4.0, "width": 0.3, "depth": 3.7, "y": -1.0, "height": 4.0 }
        ]
    }"#;
    let level = parse(json);
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    assert_no_coincident_overlaps(&all, "a wall continued at a different base");

    // Both authored walls must still contribute a visible surface: the shared
    // span once, each material on its own stretch.
    let walls: Vec<&Triangle> = all
        .iter()
        .filter(|triangle| triangle.kind == SurfaceKind::Wall)
        .collect();
    assert!(walls.iter().any(|t| t.points.iter().any(|p| p[2] < 4.0)));
    assert!(walls.iter().any(|t| t.points.iter().any(|p| p[2] > 4.4)));
}

#[test]
fn a_wall_end_abutting_another_wall_emits_no_hidden_face() {
    // Wall A crosses wall B: A's end caps at the planes of B's faces are
    // buried inside B's volume and must not be emitted at B's depth.
    let json = r#"{
        "format_version": 1,
        "id": "abutting_walls",
        "name": "Abutting Walls",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 6.0, "depth": 6.0, "height": 3.0 }],
        "walls": [
            { "x": 0.0, "z": 1.0, "width": 4.0, "depth": 0.3, "height": 3.0 },
            { "x": 1.85, "z": 0.0, "width": 0.3, "depth": 4.0, "height": 3.0 }
        ]
    }"#;
    let level = parse(json);
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    assert_no_coincident_overlaps(&all, "a wall crossing another wall");

    // The crossing is a real T: the length wall's face is interrupted by the
    // partition, and both walls still emit their own faces.
    let walls: Vec<&Triangle> = all
        .iter()
        .filter(|triangle| triangle.kind == SurfaceKind::Wall)
        .collect();
    assert!(
        walls.iter().any(|t| t.normal[2].abs() > 0.9),
        "the length wall's faces must be emitted"
    );
    assert!(
        walls.iter().any(|t| t.normal[0].abs() > 0.9),
        "the partition's faces must be emitted"
    );
}

#[test]
fn decals_are_exactly_coplanar_with_the_surface_they_mark() {
    // The decal geometry deliberately lies *on* its surface: the depth
    // relationship is settled by the decal pass's polygon offset, so the two
    // must be exactly coplanar rather than separated by an ad-hoc nudge that
    // would make the marking float.
    let json = r#"{
        "format_version": 1,
        "id": "decal_planes",
        "name": "Decal Planes",
        "spawn": { "x": 2.0, "z": 2.0 },
        "rooms": [{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }],
        "walls": [{ "x": 0.0, "z": 3.0, "width": 4.0, "depth": 0.3, "height": 3.0 }],
        "decals": [
            { "x": 2.0, "y": 0.0, "z": 2.0, "width": 0.8, "height": 0.8,
              "material": "core:decal_no_diving_01", "surface": "floor" },
            { "x": 2.0, "y": 1.5, "z": 3.0, "width": 0.8, "height": 0.8,
              "material": "core:decal_no_diving_01", "surface": "wall_north" }
        ]
    }"#;
    let level = parse(json);
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    let decals: Vec<&Triangle> = all
        .iter()
        .filter(|triangle| triangle.kind == SurfaceKind::Decal)
        .collect();
    assert_eq!(decals.len(), 4, "two decal quads");

    for decal in &decals {
        let coplanar = all.iter().any(|surface| {
            surface.kind != SurfaceKind::Decal
                && dot(surface.normal, decal.normal) > 0.999
                && (surface.offset - decal.offset).abs() < 1e-6
                && overlap_area(surface, decal) > 1e-3
        });
        assert!(
            coplanar,
            "a decal must lie exactly on a base surface at {:?}",
            decal.points[0]
        );
    }

    // The separation is the pass's fixed, sub-visible depth bias.
    assert_eq!(DECAL_POLYGON_OFFSET.0, 0.0);
    assert!(
        (-4.0..=0.0).contains(&DECAL_POLYGON_OFFSET.1),
        "the decal bias must pull towards the camera by a couple of depth steps"
    );
}

#[test]
fn intentionally_overlapping_rooms_still_emit_both_floors() {
    // Coincidence resolution must not become a blanket "delete anything that
    // overlaps" pass: two rooms a creator deliberately stacked on each other
    // keep both floors, exactly as authored.
    let json = r#"{
        "format_version": 1,
        "id": "overlap_allowed",
        "name": "Overlap Allowed",
        "spawn": { "x": 1.0, "z": 1.0 },
        "rooms": [
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 },
            { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
              "material": "core:carpet_damp_01" }
        ]
    }"#;
    let level = parse(json);
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    let floors: Vec<&Triangle> = all
        .iter()
        .filter(|triangle| triangle.kind == SurfaceKind::Floor)
        .collect();
    assert_eq!(
        floors.len(),
        4,
        "both rooms emit their own floor over the shared footprint"
    );
    let first = floors[0].material;
    assert!(
        floors.iter().any(|triangle| triangle.material != first),
        "the two authored floor materials must both survive"
    );
}

#[test]
fn the_decal_bias_is_sub_visible_at_every_practical_distance() {
    // A decal lies exactly on its base surface, so the decal pass's fixed
    // polygon offset is the entire depth separation. With OpenGL's `[-1, 1]`
    // clip convention a 24-bit window depth resolves 2^-24, and the eye-space
    // size of two of those steps is
    //   dz = z^2 * (far - near) / (far * near) * 2^-23
    // (the matrix maps `z_ndc` linearly onto the window's [0, 1] depth).
    let (near, far) = (crate::render::SCENE_NEAR_M, crate::render::SCENE_FAR_M);
    let steps = DECAL_POLYGON_OFFSET.1.abs();
    assert!(steps >= 1.0, "the bias must clear at least one depth step");
    // 2^23: half of a 24-bit depth buffer's 2^24 resolvable steps, because the
    // bias is applied in depth units but expressed here in eye space.
    let half_depth_steps = f32::from(1u16 << 15) * 256.0;
    let eye_space_bias = |z: f32| {
        let slope = (far - near) / (far * near);
        (z * z) * slope * (steps / half_depth_steps)
    };
    for (distance, limit) in [
        (1.0f32, 1e-5f32),
        (3.0, 1e-4),
        (10.0, 5e-4),
        (20.0, 2e-3),
        (50.0, 1e-2),
    ] {
        let bias = eye_space_bias(distance);
        assert!(
            bias < limit,
            "at {distance} m the bias is {bias:.6} m, past the {limit} m visibility limit"
        );
    }
    // And it is not lost in the buffer's own quantisation: two steps of a
    // 24-bit depth buffer are what the hardware resolves at these distances.
    assert!(eye_space_bias(20.0) > 0.0);
}

#[test]
fn the_shipped_demo_has_no_coincident_static_surfaces() {
    let level = parse(include_str!("../assets/levels/places_demo.json"));
    let mesh = shipped_mesh(&level);
    let all = triangles(&mesh);
    let static_only: Vec<Triangle> = all
        .iter()
        .copied()
        .filter(|triangle| triangle.kind != SurfaceKind::Decal)
        .collect();
    assert_no_coincident_overlaps(&static_only, "places_demo static surfaces");
}

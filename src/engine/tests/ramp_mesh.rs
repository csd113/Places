//! Two essential mesh regressions cover the descending-ramp deck lid and the
//! false pool-landing skirt, while checking the positive-ramp case remains
//! continuous and loader-valid partial support. Tiny source scenes build only CPU triangles with default
//! materials/lighting; no files, assets, solve, full map bake or GPU are used.

use glam::Vec3;

use super::build_level_geometry_mesh;
use crate::engine::level::{LevelDef, WalkableFloor};
use crate::engine::lighting::{LevelLighting, Visibility};
use crate::engine::loader::PropCatalog;
use crate::engine::materials::MaterialTable;

#[test]
fn ramp_replaces_carrier_and_meets_region_without_a_landing_lip() -> Result<(), String> {
    for rise in [-1.0_f32, 1.0_f32] {
        let level: LevelDef = serde_json::from_value(serde_json::json!({
            "schema":"places-level", "format_version":1_u32, "id":"ramp_mesh", "name":"Ramp mesh",
            "spawn":{"x":0.5_f32,"z":0.5_f32},
            "rooms":[{"width":4.0_f32,"depth":4.0_f32,"height":4.0_f32,"floor_y":3.0_f32,"ceiling":{"kind":"open"}}],
            "ramps":[{"x":1.0_f32,"z":1.0_f32,"width":1.0_f32,"depth":2.0_f32,"rise":rise}],
            "floor_regions":[{"x":1.0_f32,"z":3.0_f32,"width":1.0_f32,"depth":1.0_f32,"offset_y":rise}]
        })).map_err(|error| error.to_string())?;
        crate::engine::loader::validate_level(&level)?;
        let triangles = triangles_for(&level);
        let floor = WalkableFloor::from_level(&level);
        for z in [1.5_f32, 2.5_f32, 3.5_f32] {
            let heights: Vec<_> = triangles
                .iter()
                .filter(|triangle| upward(**triangle))
                .filter_map(|triangle| height_at(*triangle, 1.5_f32, z))
                .collect();
            let expected = floor
                .height_below(1.5_f32, z, 10.0_f32)
                .ok_or("Missing walking surface")?;
            assert!(
                !heights.is_empty()
                    && heights
                        .iter()
                        .all(|height| (*height - expected).abs() < 0.0001_f32),
                "each visible horizontal/slope hit must match the walking surface; no carrier deck may remain above or below the ramp"
            );
        }
        assert!(
            !triangles.iter().any(|triangle| {
                triangle
                    .iter()
                    .all(|point| (point.z - 3.0_f32).abs() < 0.0001_f32)
                    && triangle
                        .iter()
                        .any(|point| point.y < 3.0_f32 + rise.abs() - 0.01_f32)
                    && triangle
                        .iter()
                        .any(|point| point.y > 3.0_f32 + rise.min(0.0_f32) + 0.01_f32)
            }),
            "a flush ramp/region landing cannot retain a vertical carrier skirt"
        );
        assert!(
            !level.collision_aabbs().iter().any(|wall| {
                wall.min_x < 1.5_f32
                    && wall.max_x > 1.5_f32
                    && wall.min_z <= 3.0_f32
                    && wall.max_z >= 3.0_f32
                    && wall.max_y > 3.0_f32 + rise + 0.01_f32
            }),
            "ramp grid cuts must not add a blocking rim across its flush landing"
        );
        for (x, expected_normal) in [(1.0_f32, -rise.signum()), (2.0_f32, rise.signum())] {
            assert!(
                triangles.iter().any(|triangle| {
                    triangle
                        .iter()
                        .all(|point| (point.x - x).abs() < 0.0001_f32)
                        && triangle
                            .iter()
                            .all(|point| (1.0_f32..=3.0_f32).contains(&point.z))
                        && triangle
                            .iter()
                            .any(|point| (point.y - 3.0_f32).abs() > 0.01_f32)
                        && normal_x(*triangle) * expected_normal > 0.0001_f32
                }),
                "ramp side closure must face the lower walking surface for raised and recessed slopes"
            );
        }
        let visibility = Visibility::build(&level, &[]);
        assert!(
            !visibility.occludes_anywhere(
                [1.5_f32, 3.5_f32, 2.0_f32],
                [1.5_f32, rise.mul_add(0.4_f32, 3.0_f32), 2.0_f32]
            ),
            "removed carrier floor must not remain as an invisible source-visibility shadow lid"
        );
    }
    Ok(())
}

// A loader-valid partial overlap must retain its rendered slope after carrier
// subtraction. One room, one ramp and one ray add negligible CPU-only cost.
#[test]
fn partial_ramp_keeps_its_surface_when_its_centre_is_outside_the_room() -> Result<(), String> {
    let level: LevelDef = serde_json::from_value(serde_json::json!({
        "schema":"places-level", "format_version":1_u32,"id":"partial","name":"Partial",
        "spawn":{"x":0.5_f32,"z":0.5_f32},"rooms":[{"width":2.0_f32,"depth":2.0_f32,"height":3.0_f32,"ceiling":{"kind":"open"}}],
        "ramps":[{"x":1.5_f32,"z":0.5_f32,"width":2.0_f32,"depth":1.0_f32,"rise":-1.0_f32}]
    })).map_err(|error|error.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    let hits: Vec<_> = triangles_for(&level)
        .iter()
        .filter(|triangle| upward(**triangle))
        .filter_map(|triangle| height_at(*triangle, 1.75_f32, 1.0_f32))
        .collect();
    assert!(
        !hits.is_empty()
            && hits
                .iter()
                .all(|height| (*height + 0.125_f32).abs() < 0.0001_f32),
        "a supported part of a valid partial ramp must render at the physical slope, without losing its carrier and slope together"
    );
    Ok(())
}

// One four-metre raised deck protects the visible underside and
// collision backing contract that caused Annex's paper-thin exposed edges.
// CPU mesh construction only; no asset decoding, GPU or lighting solve.
#[test]
fn raised_deck_has_closed_backing_matching_collision() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{
        "schema":"places-level", "format_version":1, "id":"backing", "name":"Backing",
        "spawn":{"x":1,"z":1},
        "rooms":[{"width":4,"depth":4,"height":3,"floor_y":3,"ceiling":{"kind":"open"}}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    let mesh = triangles_for(&level);
    let underside = 3.0_f32 - crate::engine::level::MIN_STRUCTURE_M;
    assert!(
        mesh.iter().any(|triangle| {
            !upward(*triangle)
                && triangle
                    .iter()
                    .all(|p| (p.y - underside).abs() < 0.0001_f32)
                && height_at(*triangle, 1.0_f32, 1.0_f32).is_some()
        }),
        "a raised deck must have a real downward-facing underside"
    );
    assert!(
        mesh.iter().any(|triangle| {
            triangle.iter().all(|p| p.x.abs() < 0.0001_f32)
                && triangle
                    .iter()
                    .any(|p| (p.y - underside).abs() < 0.0001_f32)
                && triangle.iter().any(|p| (p.y - 3.0_f32).abs() < 0.0001_f32)
        }),
        "the exposed structural edge must connect top to underside"
    );
    let patches = WalkableFloor::from_level(&level).structural_patches();
    assert!(
        !patches.is_empty()
            && patches
                .iter()
                .all(|p| (p.backing - underside).abs() < 0.0001_f32),
        "visible structure and the rigid collision prism must share the same bottom"
    );
    Ok(())
}

fn triangles_for(level: &LevelDef) -> Vec<[Vec3; 3]> {
    let mesh = build_level_geometry_mesh(
        level,
        &PropCatalog::builtin(),
        &[],
        &LevelLighting::default(),
        &MaterialTable::default(),
    );
    mesh.ranges
        .iter()
        .flat_map(|range| {
            range
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .filter_map(|[first, second, third]| {
                    Some([
                        Vec3::from_array(range.vertices.get(usize::from(*first))?.pos),
                        Vec3::from_array(range.vertices.get(usize::from(*second))?.pos),
                        Vec3::from_array(range.vertices.get(usize::from(*third))?.pos),
                    ])
                })
        })
        .collect()
}

fn normal_x([first, second, third]: [Vec3; 3]) -> f32 {
    (second.y - first.y).mul_add(
        third.z - first.z,
        -((second.z - first.z) * (third.y - first.y)),
    )
}

fn height_at([first, second, third]: [Vec3; 3], x: f32, z: f32) -> Option<f32> {
    let area = (third.x - second.x).mul_add(
        first.z - third.z,
        (second.z - third.z) * (first.x - third.x),
    );
    if area.abs() < 0.0001_f32 {
        return None;
    }
    let first_weight =
        (third.x - second.x).mul_add(z - third.z, (second.z - third.z) * (x - third.x)) / area;
    let second_weight =
        (first.x - third.x).mul_add(z - third.z, (third.z - first.z) * (x - third.x)) / area;
    let third_weight = 1.0_f32 - first_weight - second_weight;
    [first_weight, second_weight, third_weight]
        .iter()
        .all(|weight| *weight >= -0.0001_f32)
        .then_some(third_weight.mul_add(
            third.y,
            second_weight.mul_add(second.y, first_weight * first.y),
        ))
}

fn upward(triangle: [Vec3; 3]) -> bool {
    let [a, b, c] = triangle;
    Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z)
        .cross(Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z))
        .y
        > 0.0
}

//! Tiny CPU-only mounting regressions protect non-floor-centred origins,
//! real carrier normals, back-vs-aperture contact and the millimetre clearance.
//! They need no GLB lookup, GPU or bake and finish in milliseconds.

use super::{Aabb, CLEARANCE, Transform, Vec3, ceiling_mount, corners, floor_mount, wall_mount};
use crate::engine::level::{LevelDef, LevelSurfaces};

fn scene() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{
        "schema":"places-level","format_version":1,"id":"mounting","name":"Mount fixture",
        "spawn":{"x":2,"z":2},
        "rooms":[{"x":0,"z":0,"width":8,"depth":8,"height":4}],
        "walls":[{"x":0,"y":0,"z":0,"width":8,"depth":0.2,"height":4}],
        "ramps":[{"x":1,"z":1,"width":4,"depth":2,"rise":1}]
    }"#,
    )
    .map_err(|error| error.to_string())
}

fn pose(position: [f32; 3]) -> Transform {
    Transform {
        position,
        size: [1.0; 3],
        yaw_degrees: 31.0,
        pitch_degrees: 0.0,
        roll_degrees: 0.0,
    }
}

#[test]
fn mounting_uses_actual_base_top_and_back_with_carrier_normal() -> Result<(), String> {
    let level = scene()?;
    let local = Aabb {
        min: [-0.3, 0.12, -0.2],
        max: [0.3, 0.8, 0.2],
    };
    let mounted = floor_mount(&level, pose([2.0, -0.5, 2.0]), local, None)?;
    let rotation = glam::Quat::from_euler(
        glam::EulerRot::YXZ,
        mounted.yaw_degrees.to_radians(),
        mounted.pitch_degrees.to_radians(),
        mounted.roll_degrees.to_radians(),
    );
    let expected_normal = Vec3::new(-0.25, 1.0, 0.0).normalize();
    assert!(
        (rotation * Vec3::Y).distance(expected_normal) < 0.00001,
        "floor mounting aligns the model's actual up with the ramp normal"
    );
    let surfaces = LevelSurfaces::new(&level);
    let floor = surfaces.floor_y_at(2.0, 2.0).ok_or("No fixture floor")?;
    for local_point in corners(local) {
        let world = Vec3::new(
            mounted.position[0],
            floor + mounted.position[1],
            mounted.position[2],
        ) + rotation * local_point;
        let support = surfaces.floor_y_at(world.x, world.z).ok_or("No support")?;
        if (local_point.y - local.min[1]).abs() < f32::EPSILON {
            assert!(
                (world.y - support - CLEARANCE).abs() < 0.00001,
                "all four base corners contact the true sloped floor"
            );
        }
    }
    let ceiling = ceiling_mount(
        &level,
        pose([2.0, 3.0, 4.0]),
        Aabb {
            min: [-0.25, -0.03, -0.25],
            max: [0.25, 0.003, 0.25],
        },
        None,
    )?;
    assert!(
        (ceiling.position[1] + 0.003 + CLEARANCE - 4.0).abs() < 0.00001,
        "the protruding mounting top, not the luminous disc, touches the ceiling"
    );
    let wall = wall_mount(
        &level,
        pose([2.0, 2.0, 0.8]),
        Aabb {
            min: [-0.2, -0.1, 0.002],
            max: [0.2, 0.1, 0.113],
        },
        None,
    )?;
    assert!(
        wall.yaw_degrees.abs() < 0.00001,
        "the wall's interior face aims local +Z into the room"
    );
    assert!(
        (wall.position[2] + 0.002 - 0.2 - CLEARANCE).abs() < 0.00001,
        "the true back plane, not the luminous face, receives clearance"
    );
    let mut stepped = level.clone();
    stepped.floor_regions.push(
        serde_json::from_value(serde_json::json!({
            "x":2.1_f32,"z":3.0_f32,"width":1.0_f32,"depth":2.0_f32,"offset_y":0.5_f32
        }))
        .map_err(|error| error.to_string())?,
    );
    assert!(
        floor_mount(&stepped, pose([2.0, 0.0, 4.0]), local, None).is_err(),
        "mounting refuses a footprint spanning two incompatible support heights instead of leaving a visibly floating base"
    );
    let mut stacked = level.clone();
    let mut upper = stacked
        .rooms
        .first()
        .ok_or("Missing floor fixture")?
        .clone();
    upper.floor_y = 3.0;
    stacked.rooms.push(upper);
    let upper_mount = floor_mount(&stacked, pose([2.0, 2.9, 4.0]), local, None)?;
    assert!(
        (upper_mount.position[1] + local.min[1] - 3.0 - CLEARANCE).abs() < 0.00001,
        "floor mounting retains the nearest authored storey while source Y remains relative to the first carrier"
    );
    let mut gabled = level.clone();
    let room = gabled.rooms.first_mut().ok_or("Missing ceiling fixture")?;
    room.ceiling = crate::engine::level::CeilingProfileDef::Gable {
        ridge: crate::engine::level::WallAxis::X,
        ridge_rise: 1.0,
    };
    assert!(
        ceiling_mount(&gabled, pose([2.0, 3.0, 4.0]), local, None).is_err(),
        "a yaw-only fixture cannot silently float under an unrepresentable sloped mount"
    );
    Ok(())
}

#[test]
fn mounted_panel_is_not_reported_as_a_duplicate_surface() -> Result<(), String> {
    // Closing Time exposed an actual mount/checker conflict: the old 1 mm
    // clearance was reported as a confirmed duplicate. Two tiny mesh checks
    // protect the shared operation without assets, lighting or a map bake.
    let mut mounted_scene = scene()?;
    mounted_scene.ramps.clear();
    let panel = ceiling_mount(
        &mounted_scene,
        pose([4.0, 3.0, 4.0]),
        Aabb {
            min: [-0.6, -0.1, -0.3],
            max: [0.6, 0.0, 0.3],
        },
        None,
    )?;
    mounted_scene.ceiling_lights.push(
        serde_json::from_value(serde_json::json!({
            "fixture":"core:fluorescent_panel_01","x":4.0_f32,"z":4.0_f32,
            "y":panel.position[1],"align":"none"
        }))
        .map_err(|error| error.to_string())?,
    );
    let mounted_report =
        crate::map_compiler::geometry_check::check_level(&mounted_scene, "mounted panel", true);
    assert!(
        !mounted_report
            .findings
            .iter()
            .any(|finding| finding.check == "duplicate-surface"),
        "an ordinary mounted fixture must pass the production coplanarity checker"
    );
    mounted_scene
        .ceiling_lights
        .first_mut()
        .ok_or("Missing panel")?
        .y = Some(4.0);
    let touching_report =
        crate::map_compiler::geometry_check::check_level(&mounted_scene, "touching panel", true);
    assert!(
        touching_report
            .findings
            .iter()
            .any(|finding| finding.check == "duplicate-surface"),
        "the regression must still detect a genuinely coincident fixture face"
    );
    Ok(())
}

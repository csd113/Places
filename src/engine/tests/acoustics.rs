//! Small CPU regressions protect doorway geometry, measured direction spectra,
//! material decay and untrusted coefficients. No maps are baked and no device
//! opens; fixtures use at most two rooms or eight fixed cabins.
use crate::engine::audio::acoustics::{AcousticMaterial, AcousticScene};
use crate::engine::door::Doors;
use crate::engine::level::LevelDef;
use glam::Vec3;

fn fixture() -> Result<LevelDef, String> {
    LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"acoustics","name":"Acoustics","spawn":{"x":1,"z":1},"rooms":[{"x":0,"z":0,"width":8,"depth":10,"height":3}],"walls":[{"x":0,"z":5,"width":8,"depth":0.2,"height":3,"openings":[{"offset":3,"width":2,"height":2.5}]}],"doors":[{"id":"door","x":3,"z":5.1,"width":2,"height":2.5,"open_speed_degrees":180}]}"#).map_err(|error|error.to_string())
}
#[test]
fn sliced_wall_and_live_oriented_door_invalidate_paths() -> Result<(), String> {
    let level = fixture()?;
    let mut scene = AcousticScene::build(&level);
    let listener = Vec3::new(4.0, 1.6, 3.0);
    let source = Vec3::new(4.0, 1.6, 7.0);
    let closed = scene.path(listener, source);
    assert!(
        closed.transmission < 0.3 && closed.cutoff_hz < 3000.0,
        "closed leaf must attenuate and muffle a genuine opening"
    );
    let mut open_level = level.clone();
    open_level.doors.clear();
    let open = AcousticScene::build(&open_level).path(listener, source);
    assert!(
        open.transmission > 0.99,
        "wall slices must preserve the actual doorway"
    );
    let wall = scene.path(Vec3::new(1.0, 1.6, 3.0), Vec3::new(1.0, 1.6, 7.0));
    assert!(
        wall.transmission < 0.3,
        "solid wall must obstruct the same ray away from the doorway"
    );
    let mut partition_level = open_level.clone();
    partition_level.walls.clear();
    partition_level.half_walls.push(
        serde_json::from_value(
            serde_json::json!({"x":1_f32,"z":5_f32,"width":1_f32,"depth":0.2_f32,"height":2.5_f32}),
        )
        .map_err(|error| error.to_string())?,
    );
    assert!(
        AcousticScene::build(&partition_level)
            .path(Vec3::new(1.5, 1.6, 3.0), Vec3::new(1.5, 1.6, 7.0))
            .transmission
            < 0.3,
        "solid authored partitions must join the cached scene too"
    );
    // A solid pane also appears in canonical collision. Matching shared bounds
    // must deduplicate it, or its authored transmission is applied twice.
    let mut glazed = open_level.clone();
    let opening = glazed
        .walls
        .first_mut()
        .and_then(|section| section.openings.first_mut())
        .ok_or("glazed fixture opening")?;
    opening.glass = Some("core:glass_clear_01".into());
    opening.solid = true;
    let pane = AcousticScene::build(&glazed).path(listener, source);
    assert!(
        (pane.transmission - 0.2).abs() < 0.0001,
        "one physical pane must apply its material exactly once"
    );
    let mut doors = Doors::from_level(&level);
    let _opened = doors.request_open("door");
    let _moved = doors.advance(1.0, |_, _| false);
    let revision = scene.revision;
    scene.update_doors(&doors);
    assert!(
        scene.revision != revision
            && scene.path(listener, source).transmission > closed.transmission,
        "changing the real leaf pose must invalidate and open the path"
    );
    Ok(())
}
#[test]
fn material_size_and_openings_change_decay_and_authored_values_validate() -> Result<(), String> {
    let mut level = fixture()?;
    level.walls.clear();
    level.doors.clear();
    level.defaults.floor = "core:carpet_beige_01".into();
    level.defaults.ceiling = "core:ceiling_tile_01".into();
    level.defaults.wall = "core:acoustic_wall".into();
    let office = AcousticScene::build(&level).room_at(Vec3::new(2.0, 1.6, 2.0));
    level.defaults.floor = "core:pool_tile_01".into();
    level.defaults.ceiling = "core:pool_tile_01".into();
    level.defaults.wall = "core:pool_tile_01".into();
    let pool = AcousticScene::build(&level).room_at(Vec3::new(2.0, 1.6, 2.0));
    assert!(
        pool.decay_seconds > office.decay_seconds * 2.0 && office.damping > pool.damping,
        "hard room must retain substantially more high-frequency energy and decay"
    );
    assert_audio_metadata_preserves_render_slots(&mut level)?;
    level
        .acoustic_materials
        .push(crate::engine::audio::acoustics::AcousticMaterialDef {
            material: "core:pool_tile_01".into(),
            absorption: [0.8, 0.04, 0.08],
            transmission: 0.2,
        });
    assert!(
        AcousticScene::build(&level)
            .room_at(Vec3::new(2.0, 1.6, 2.0))
            .decay_seconds
            < pool.decay_seconds,
        "authored low-band absorption must affect the room response too"
    );
    level.acoustic_materials.clear();
    let room = level.rooms.first_mut().ok_or("room")?;
    room.width *= 2.0;
    room.depth *= 2.0;
    let large = AcousticScene::build(&level).room_at(Vec3::new(2.0, 1.6, 2.0));
    assert!(
        large.decay_seconds >= pool.decay_seconds,
        "greater room volume must influence decay"
    );
    assert!(
        AcousticMaterial {
            absorption: [0.1, f32::NAN, 0.5],
            transmission: 0.2
        }
        .validate()
        .is_err(),
        "nonfinite authored absorption must reject before runtime"
    );
    assert!(
        serde_json::from_str::<AcousticMaterial>(r#"{"absorbtion":[0,0,0]}"#).is_err(),
        "misspelled acoustic fields must be explicit errors"
    );
    let encoded = serde_json::to_string(&level).map_err(|error| error.to_string())?;
    let reopened = LevelDef::from_json(&encoded).map_err(|error| error.to_string())?;
    assert_eq!(
        reopened
            .rooms
            .first()
            .map(|reopened_room| reopened_room.acoustics),
        level
            .rooms
            .first()
            .map(|reopened_room| reopened_room.acoustics),
        "room authoring must survive a typed roundtrip"
    );
    Ok(())
}

fn assert_audio_metadata_preserves_render_slots(level: &mut LevelDef) -> Result<(), String> {
    let render_ids = crate::engine::materials::referenced_material_ids(level);
    let lighting_key = crate::map_compiler::lighting_fingerprint_with_revision(level, &[], &[], 1)?;
    level
        .acoustic_materials
        .push(crate::engine::audio::acoustics::AcousticMaterialDef {
            material: "core:carpet_beige_01".into(),
            absorption: [0.2, 0.6, 0.9],
            transmission: 0.1,
        });
    assert_eq!(
        crate::engine::materials::referenced_material_ids(level),
        render_ids,
        "audio-only material authoring must not allocate or reorder render slots"
    );
    assert_eq!(
        crate::map_compiler::lighting_fingerprint_with_revision(level, &[], &[], 1)?,
        lighting_key,
        "audio-only edits must preserve solved lighting cache identity"
    );
    Ok(())
}

// These small no-bake fixtures reproduce missing horizontal/elevator barriers
// and cross-storey reverb contamination. Each uses at most two rooms or eight
// cabins; they protect geometry contracts unavailable in the former wall test.
fn bare_fixture() -> Result<LevelDef, String> {
    let mut level = fixture()?;
    level.walls.clear();
    level.doors.clear();
    Ok(level)
}
#[test]
fn horizontal_surfaces_preserve_storeys_regions_ramps_and_real_open_space() -> Result<(), String> {
    let mut level = bare_fixture()?;
    let lower = level.rooms.first().ok_or("lower room")?.clone();
    let mut upper = lower;
    upper.floor_y = 3.5;
    level.rooms.push(upper);
    let ray = (Vec3::new(2.0, 1.6, 2.0), Vec3::new(2.0, 5.1, 2.0));
    let path = AcousticScene::build(&level).path(ray.0, ray.1);
    assert!(path.transmission < 0.3 && path.cutoff_hz < 3000.0);
    assert!(
        AcousticScene::build(&level)
            .path(ray.0, Vec3::new(6.0, 1.6, 8.0))
            .transmission
            > 0.99
    );
    // Room footprints are real finite surfaces, not infinite storey planes.
    assert!(
        AcousticScene::build(&level)
            .path(Vec3::new(9.0, 1.6, 2.0), Vec3::new(9.0, 5.1, 2.0))
            .transmission
            > 0.99
    );
    level.rooms.truncate(1);
    level.rooms.first_mut().ok_or("room")?.ceiling =
        serde_json::from_str(r#"{"kind":"open"}"#).map_err(|e| e.to_string())?;
    assert!(AcousticScene::build(&level).path(ray.0, ray.1).transmission > 0.99);
    level.floor_regions.push(
        serde_json::from_value(
            serde_json::json!({"x":1_f32,"z":1_f32,"width":2_f32,"depth":2_f32,"offset_y":-2_f32}),
        )
        .map_err(|e| e.to_string())?,
    );
    assert!(
        AcousticScene::build(&level)
            .path(Vec3::new(2.0, -1.0, 2.0), Vec3::new(2.0, 1.0, 2.0))
            .transmission
            > 0.99,
        "a recessed carrier must leave its former floor plane clear"
    );
    assert!(
        AcousticScene::build(&level)
            .path(Vec3::new(2.0, -3.0, 2.0), Vec3::new(2.0, -1.0, 2.0))
            .transmission
            < 0.3
    );
    level.floor_regions.clear();
    level.ramps.push(
        serde_json::from_value(
            serde_json::json!({"x":1_f32,"z":1_f32,"width":4_f32,"depth":2_f32,"rise":-2_f32}),
        )
        .map_err(|e| e.to_string())?,
    );
    let scene = AcousticScene::build(&level);
    assert!(
        scene
            .path(Vec3::new(3.0, -0.5, 2.0), Vec3::new(3.0, 0.5, 2.0))
            .transmission
            > 0.99,
        "the removed carrier cannot seal the space above a descending slope"
    );
    assert!(
        scene
            .path(Vec3::new(3.0, -1.5, 2.0), Vec3::new(3.0, -0.5, 2.0))
            .transmission
            < 0.3
    );
    level.ramps.clear();
    level.rooms.first_mut().ok_or("room")?.ceiling =
        serde_json::from_str(r#"{"kind":"gable","ridge":"x","ridge_rise":2}"#)
            .map_err(|e| e.to_string())?;
    let room = level.rooms.first().ok_or("room")?;
    let y = room.ceiling_y_at(2.0, 2.0);
    let pitched = AcousticScene::build(&level);
    assert!(
        pitched
            .path(Vec3::new(2.0, y - 0.2, 2.0), Vec3::new(2.0, y + 0.2, 2.0))
            .transmission
            < 0.3
    );
    assert!(
        pitched
            .path(Vec3::new(2.0, y - 0.4, 2.0), Vec3::new(2.0, y - 0.1, 2.0))
            .transmission
            > 0.99
    );
    assert_real_stair_shaft_and_material_ownership()
}

fn assert_real_stair_shaft_and_material_ownership() -> Result<(), String> {
    let mut level = bare_fixture()?;
    let lower = level.rooms.first_mut().ok_or("room")?;
    lower.height = 7.0;
    lower.material = Some("tile".into());
    lower.ceiling = serde_json::from_str(r#"{"kind":"open"}"#).map_err(|e| e.to_string())?;
    let mut upper = lower.clone();
    upper.x = 4.0;
    upper.width = 4.0;
    upper.floor_y = 3.5;
    level.rooms.push(upper);
    level.stairs.push(
        serde_json::from_value(serde_json::json!({
            "x":0_f32,"z":0_f32,"width":4_f32,"depth":2_f32,"rise":3.5_f32,"steps":10_u8
        }))
        .map_err(|e| e.to_string())?,
    );
    let scene = AcousticScene::build(&level);
    assert!(
        scene
            .path(Vec3::new(2.0, 1.6, 5.0), Vec3::new(2.0, 5.1, 5.0))
            .transmission
            > 0.99,
        "a real shaft cut from the upper footprint and open lower ceiling remains clear"
    );
    assert!(
        scene
            .path(Vec3::new(6.0, 1.6, 5.0), Vec3::new(6.0, 5.1, 5.0))
            .transmission
            < 0.3
    );
    assert!(
        scene
            .path(Vec3::new(2.0, 1.0, 1.0), Vec3::new(2.0, 3.0, 1.0))
            .transmission
            < 0.3,
        "the actual stair tread and its backing still block"
    );
    level.stairs.clear();
    level.rooms.truncate(1);
    level.ramps.push(
        serde_json::from_value(serde_json::json!({
            "x":1_f32,"z":1_f32,"width":4_f32,"depth":2_f32,"rise":-2_f32
        }))
        .map_err(|e| e.to_string())?,
    );
    level.floor_patches.push(
        serde_json::from_value(serde_json::json!({
            "x":1_f32,"z":1_f32,"width":4_f32,"depth":2_f32,"material":"carpet"
        }))
        .map_err(|e| e.to_string())?,
    );
    let ramp =
        AcousticScene::build(&level).path(Vec3::new(3.0, -1.5, 2.0), Vec3::new(3.0, -0.5, 2.0));
    assert!(
        (ramp.transmission - 0.2).abs() < 0.0001,
        "an unpainted ramp inherits its room material, not the patch on its removed carrier"
    );
    Ok(())
}
#[test]
fn coincident_floor_ceiling_and_solid_are_one_acoustic_barrier() -> Result<(), String> {
    let mut level = bare_fixture()?;
    let ray = (Vec3::new(2.0, -1.0, 2.0), Vec3::new(2.0, 1.0, 2.0));
    let a = AcousticScene::build(&level).path(ray.0, ray.1);
    let room = level.rooms.first().ok_or("room")?.clone();
    level.rooms.push(room);
    let b = AcousticScene::build(&level).path(ray.0, ray.1);
    assert_eq!(
        a.transmission, b.transmission,
        "overlapping room ownership cannot multiply one interface"
    );
    level.void_walls.push(
        serde_json::from_value(
            serde_json::json!({"min":[0_f32,-0.1_f32,0_f32],"max":[8_f32,0.1_f32,10_f32],"material":"wall","solid":true}),
        )
        .map_err(|e| e.to_string())?,
    );
    let c = AcousticScene::build(&level).path(ray.0, ray.1);
    assert_eq!(
        a.transmission, c.transmission,
        "a carrier inside its physical slab applies once"
    );
    Ok(())
}
#[test]
fn shared_elevator_poses_cover_apertures_shells_rotations_and_retirement() -> Result<(), String> {
    use crate::engine::elevator::{Elevator, Phase};
    for rotation in [0.0_f32, 90.0, 37.0] {
        let mut level = bare_fixture()?;
        let def=serde_json::from_value(serde_json::json!({"id":"lift","x":4_f32,"z":4_f32,"rotation_degrees":rotation,"stops":[{"label":"lower","y":0_f32},{"label":"upper","y":4_f32}]})).map_err(|e|e.to_string())?;
        let mut lift = Elevator::new(&def);
        level.elevators.push(def);
        let mut scene = AcousticScene::build(&level);
        let point = |x, y, z| {
            level
                .elevators
                .first()
                .map_or(Vec3::ZERO, |entry| entry.point(x, y, z))
        };
        let ray = (point(0.0, 1.6, 1.2), point(0.0, 1.6, -1.0));
        let closed = scene.path(ray.0, ray.1);
        assert!(closed.transmission < 0.3);
        assert_eq!(closed.transmission, scene.path(ray.1, ray.0).transmission);
        let revision = scene.revision;
        scene.update_elevators(std::slice::from_ref(&lift));
        assert_eq!(revision, scene.revision);
        for opening in [0.5_f32, 1.0] {
            lift.opening = opening;
            lift.phase = Phase::Open;
            scene.update_elevators(std::slice::from_ref(&lift));
            assert!(
                scene.path(ray.0, ray.1).transmission > 0.99,
                "central open aperture must clear at rotation {rotation}"
            );
            assert!(
                scene
                    .path(point(0.0, 1.6, 1.2), point(2.0, 1.6, 1.2))
                    .transmission
                    < 0.3
            );
            assert!(
                scene
                    .path(point(0.0, 1.6, 1.2), point(0.0, 1.6, 3.0))
                    .transmission
                    < 0.3
            );
        }
        lift.opening = 0.2;
        scene.update_elevators(std::slice::from_ref(&lift));
        assert!(
            scene
                .path(point(0.5, 1.6, 1.2), point(0.5, 1.6, -1.0))
                .transmission
                < 0.3,
            "remaining partial leaf still blocks its own ray"
        );
        lift.opening = 0.0;
        lift.phase = Phase::Moving;
        scene.update_elevators(std::slice::from_ref(&lift));
        assert!(scene.path(ray.0, ray.1).transmission < 0.3);
        lift.at = 1;
        lift.y = 4.0;
        lift.opening = 1.0;
        lift.phase = Phase::Open;
        scene.update_elevators(std::slice::from_ref(&lift));
        assert!(
            scene.path(ray.0, ray.1).transmission < 0.3,
            "departure landing stays sealed"
        );
        assert!(
            scene
                .path(point(0.0, 5.6, 1.2), point(0.0, 5.6, -1.0))
                .transmission
                > 0.99
        );
        scene.update_elevators(&[]);
        assert!(
            scene.path(ray.0, ray.1).transmission > 0.99,
            "retired dynamic geometry cannot remain after unload"
        );
    }
    assert_maximum_cabins()
}

fn assert_maximum_cabins() -> Result<(), String> {
    use crate::engine::elevator::Elevator;
    let mut maximum = bare_fixture()?;
    for index in 0_u8..8 {
        maximum.elevators.push(
            serde_json::from_value(serde_json::json!({
                "id":format!("lift_{index}"),"x":f32::from(index)*4.0,"z":4_f32,
                "stops":[{"label":"landing","y":0_f32}]
            }))
            .map_err(|e| e.to_string())?,
        );
    }
    let mut scene = AcousticScene::build(&maximum);
    let lifts: Vec<_> = maximum.elevators.iter().map(Elevator::new).collect();
    let revision = scene.revision;
    scene.update_elevators(&lifts);
    assert_eq!(
        revision, scene.revision,
        "maximum valid cabins must reuse stable pose storage"
    );
    for lift in &lifts {
        assert!(
            scene
                .path(
                    lift.def.point(0.0, 1.6, 1.2),
                    lift.def.point(0.0, 1.6, -1.0)
                )
                .transmission
                < 0.3
        );
    }
    Ok(())
}
#[test]
fn room_material_and_apertures_clip_to_actual_storey_and_opening_extent() -> Result<(), String> {
    let mut level = bare_fixture()?;
    level.defaults.floor = "tile".into();
    level.defaults.ceiling = "tile".into();
    level.defaults.wall = "tile".into();
    let position = Vec3::new(2.0, 1.5, 2.0);
    let baseline = AcousticScene::build(&level).room_at(position);
    let unrelated=serde_json::from_value(serde_json::json!({"x":-4_f32,"y":20_f32,"z":5_f32,"width":12_f32,"depth":0.2_f32,"height":3_f32,"material":"carpet","openings":[{"offset":5_f32,"width":2_f32,"height":2.5_f32}]})).map_err(|e|e.to_string())?;
    level.walls.push(unrelated);
    assert_eq!(baseline, AcousticScene::build(&level).room_at(position));
    level.walls.first_mut().ok_or("wall")?.y = 0.0;
    let inside = AcousticScene::build(&level).room_at(position);
    assert_ne!(
        baseline, inside,
        "a wall starting outside the room contributes its actual overlapping face/opening"
    );
    level.walls.first_mut().ok_or("wall")?.y = 3.0;
    assert_eq!(
        baseline,
        AcousticScene::build(&level).room_at(position),
        "a touching storey has no overlapping face height"
    );
    level.walls.first_mut().ok_or("wall")?.y = 0.0;
    level
        .walls
        .first_mut()
        .and_then(|entry| entry.openings.first_mut())
        .ok_or("opening")?
        .sill = 10.0;
    let outside_opening = AcousticScene::build(&level).room_at(position);
    level.walls.first_mut().ok_or("wall")?.openings.clear();
    assert_eq!(
        outside_opening,
        AcousticScene::build(&level).room_at(position),
        "an in-room origin cannot admit an opening above its ceiling"
    );
    let wall = level.walls.first_mut().ok_or("wall")?;
    wall.x += wall.width;
    wall.width = -wall.width;
    assert_eq!(
        outside_opening,
        AcousticScene::build(&level).room_at(position),
        "normalized dimensions preserve the actual face"
    );
    level.walls.first_mut().ok_or("wall")?.y = -2.0;
    level.walls.first_mut().ok_or("wall")?.height = Some(3.0);
    let partial = AcousticScene::build(&level).room_at(position);
    let clipped_wall = level.walls.first_mut().ok_or("wall")?;
    clipped_wall.x = 0.0;
    clipped_wall.width = 8.0;
    clipped_wall.y = 0.0;
    clipped_wall.height = Some(1.0);
    assert_eq!(
        partial,
        AcousticScene::build(&level).room_at(position),
        "only actual clipped horizontal and vertical face overlap may affect this room"
    );
    let before_patch = AcousticScene::build(&level).room_at(position);
    level.floor_patches.push(
        serde_json::from_value(serde_json::json!({
            "x":4_f32,"z":4_f32,"width":-2_f32,"depth":2_f32,"material":"carpet"
        }))
        .map_err(|e| e.to_string())?,
    );
    assert_eq!(
        before_patch,
        AcousticScene::build(&level).room_at(position),
        "a malformed patch skipped by the floor emitter cannot contribute acoustic material"
    );
    Ok(())
}

//! Small CPU regressions protect doorway geometry, measured direction spectra,
//! material decay and untrusted coefficients. No maps are baked and no device
//! opens; total fixture size is one room, a sliced wall and one leaf.
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

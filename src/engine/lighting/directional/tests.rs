//! Authoring, vertex fallback and compiled-record contracts.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]
use super::*;
use crate::engine::level::LevelDef;

fn level() -> LevelDef {
    LevelDef::from_json(
        r#"{"format_version":3,"id":"moon_test","name":"Moon test",
        "spawn":{"x":1,"z":1},"rooms":[{"x":0,"z":0,"width":4,"depth":4,"height":3}],
        "global_illuminators":[{"id":"moon","kind":"directional","direction":[0,-1,0],
            "color":[0.85,0.9,1],"intensity":0.12,"angular_size_degrees":0}] }"#,
    )
    .expect("fixture")
}

#[test]
fn authoring_rejects_invalid_vectors_energy_and_duplicate_ids() {
    let valid = level();
    crate::engine::loader::validate_level(&valid).expect("valid moon");
    for direction in [[0.0; 3], [f32::NAN, -1.0, 0.0], [f32::INFINITY, 0.0, 0.0]] {
        let mut invalid = valid.clone();
        invalid.global_illuminators[0].direction = direction;
        assert!(crate::engine::loader::validate_level(&invalid).is_err());
    }
    let mut duplicate = valid.clone();
    duplicate
        .global_illuminators
        .push(valid.global_illuminators[0].clone());
    assert!(crate::engine::loader::validate_level(&duplicate).is_err());
    for intensity in [-1.0, f32::NAN, 8.1] {
        let mut invalid = valid.clone();
        invalid.global_illuminators[0].intensity = intensity;
        assert!(crate::engine::loader::validate_level(&invalid).is_err());
    }
    let mut extreme = valid.global_illuminators[0].clone();
    extreme.direction = [f32::MAX, -f32::MAX, 0.0];
    assert!(
        DirectionalLight::from_definition(&extreme)
            .expect("finite normalization")
            .is_valid()
    );
}

#[test]
fn vertex_fallback_and_compiled_record_preserve_closed_roof_isolation() {
    let mut level = level();
    let enclosed = crate::engine::lighting::LevelLighting::bake(&level);
    assert_eq!(
        enclosed.global_surface_light([2.0, 0.0, 2.0], [0.0, 1.0, 0.0]),
        crate::engine::lighting::LightColor::BLACK
    );
    level.rooms[0].ceiling = crate::engine::level::CeilingProfileDef::Open;
    let outdoor = crate::engine::lighting::LevelLighting::bake(&level);
    let light = outdoor.global_surface_light([2.0, 0.0, 2.0], [0.0, 1.0, 0.0]);
    assert!((light.b - 0.12).abs() < 1.0e-6);
    assert_eq!(
        outdoor.global_surface_light([2.0, 0.0, 2.0], [0.0, -1.0, 0.0]),
        crate::engine::lighting::LightColor::BLACK
    );
    let mut writer = Writer::new();
    outdoor.write_compiled(&mut writer).expect("write");
    let bytes = writer.into_bytes();
    let decoded = crate::engine::lighting::LevelLighting::read_compiled(&mut Reader::new(&bytes))
        .expect("read");
    assert_eq!(
        light,
        decoded.global_surface_light([2.0, 0.0, 2.0], [0.0, 1.0, 0.0])
    );
}

#[test]
fn inactive_or_unbaked_sources_contribute_nothing() {
    let definition = level().global_illuminators.remove(0);
    for (enabled, bake) in [(false, true), (true, false)] {
        let mut inactive = definition.clone();
        inactive.enabled = enabled;
        inactive.bake = bake;
        assert!(DirectionalLight::from_definition(&inactive).is_none());
    }
}

/// Five cheap visibility queries on one tiny room protect Static sky energy
/// and roof isolation. The codec check prevents a reload from losing that sky.
#[test]
fn static_sky_is_visible_outdoors_and_survives_the_lighting_record() {
    let mut authored = level();
    authored.global_illuminators.clear();
    authored.sky = Some(crate::engine::level::SkyDef {
        texture: "unused:test_sky".to_string(),
        brightness: 4.0,
        ambient: 0.25,
        ambient_color: Some([0.2, 0.4, 0.8]),
    });
    let enclosed = crate::engine::lighting::LevelLighting::bake(&authored);
    assert_eq!(
        enclosed.global_surface_light([2.0, 2.5, 2.0], [0.0, 1.0, 0.0]),
        crate::engine::lighting::LightColor::BLACK
    );
    authored.rooms[0].ceiling = crate::engine::level::CeilingProfileDef::Open;
    let outdoor = crate::engine::lighting::LevelLighting::bake(&authored);
    let sky = outdoor.global_surface_light([2.0, 2.5, 2.0], [0.0, 1.0, 0.0]);
    assert_eq!(sky.to_array(), [0.05, 0.1, 0.2]);
    let mut writer = Writer::new();
    outdoor.write_compiled(&mut writer).expect("encode sky");
    let bytes = writer.into_bytes();
    let decoded = crate::engine::lighting::LevelLighting::read_compiled(&mut Reader::new(&bytes))
        .expect("decode sky");
    assert_eq!(
        decoded.global_surface_light([2.0, 2.5, 2.0], [0.0, 1.0, 0.0]),
        sky
    );
    authored.sky.as_mut().expect("sky").ambient = 0.0;
    assert_eq!(
        crate::engine::lighting::LevelLighting::bake(&authored)
            .global_surface_light([2.0, 2.5, 2.0], [0.0, 1.0, 0.0]),
        crate::engine::lighting::LightColor::BLACK
    );
}

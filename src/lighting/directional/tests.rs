//! Authoring, vertex fallback and compiled-record contracts.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::float_cmp)]
use super::*;
use crate::level::LevelDef;

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
    crate::loader::validate_level(&valid).expect("valid moon");
    for direction in [[0.0; 3], [f32::NAN, -1.0, 0.0], [f32::INFINITY, 0.0, 0.0]] {
        let mut invalid = valid.clone();
        invalid.global_illuminators[0].direction = direction;
        assert!(crate::loader::validate_level(&invalid).is_err());
    }
    let mut duplicate = valid.clone();
    duplicate
        .global_illuminators
        .push(valid.global_illuminators[0].clone());
    assert!(crate::loader::validate_level(&duplicate).is_err());
    for intensity in [-1.0, f32::NAN, 8.1] {
        let mut invalid = valid.clone();
        invalid.global_illuminators[0].intensity = intensity;
        assert!(crate::loader::validate_level(&invalid).is_err());
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
    let enclosed = crate::lighting::LevelLighting::bake(&level);
    assert_eq!(
        enclosed.global_surface_light([2.0, 0.0, 2.0], [0.0, 1.0, 0.0]),
        crate::lighting::LightColor::BLACK
    );
    level.rooms[0].ceiling = crate::level::CeilingProfileDef::Open;
    let outdoor = crate::lighting::LevelLighting::bake(&level);
    let light = outdoor.global_surface_light([2.0, 0.0, 2.0], [0.0, 1.0, 0.0]);
    assert!((light.b - 0.12).abs() < 1.0e-6);
    assert_eq!(
        outdoor.global_surface_light([2.0, 0.0, 2.0], [0.0, -1.0, 0.0]),
        crate::lighting::LightColor::BLACK
    );
    let mut writer = Writer::new();
    outdoor.write_compiled(&mut writer).expect("write");
    let bytes = writer.into_bytes();
    let decoded =
        crate::lighting::LevelLighting::read_compiled(&mut Reader::new(&bytes)).expect("read");
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

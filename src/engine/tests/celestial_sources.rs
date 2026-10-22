//! Protects the visible-source/light sign and old-scene serialization contract.
//! One tiny CPU-only source fixture: no filesystem, GPU, image decode or bake.
use crate::engine::level::{CelestialVisualDef, GlobalIlluminatorDef};
use crate::engine::lighting::directional::DirectionalLight;

#[test]
fn celestial_position_and_light_share_one_round_trippable_direction() -> Result<(), String> {
    let mut source: GlobalIlluminatorDef =
        serde_json::from_str(r#"{"id":"sun","kind":"sun","direction":[0,-1,0]}"#)
            .map_err(|error| error.to_string())?;
    assert!(
        source.visual.is_none(),
        "old scenes must not gain a new disc"
    );
    assert!(
        !serde_json::to_value(&source)
            .map_err(|error| error.to_string())?
            .as_object()
            .ok_or("source object")?
            .contains_key("visual")
    );
    source.visual = Some(CelestialVisualDef {
        angular_size_degrees: 6.0,
        brightness: 1.0,
    });
    for (bearing, elevation) in [
        (0.0, 30.0),
        (90.0, 45.0),
        (180.0, 15.0),
        (270.0, 60.0),
        (360.0, 0.0),
        (45.0, 90.0),
        (135.0, -20.0),
    ] {
        source.direction = GlobalIlluminatorDef::direction_from_angles(bearing, elevation)?;
        source.validate()?;
        let incoming = DirectionalLight::from_definition(&source)
            .ok_or("baked source absent")?
            .incoming;
        assert_eq!(
            incoming,
            source.source_direction(),
            "visible position and bake must use the same sign and normalization"
        );
        let angles = source.celestial_angles();
        let reconstructed = GlobalIlluminatorDef::direction_from_angles(angles[0], angles[1])?;
        assert!(
            glam::Vec3::from_array(source.direction)
                .distance(glam::Vec3::from_array(reconstructed))
                < 1.0e-5
        );
        let reopened: GlobalIlluminatorDef = serde_json::from_value(
            serde_json::to_value(&source).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(reopened.direction, source.direction);
        assert_eq!(reopened.visual_texture(), Some("core:celestial_sun"));
    }
    let stage_key = |level: &crate::engine::level::LevelDef| {
        crate::map_compiler::lighting_fingerprint_with_revision(
            level,
            &[crate::engine::quality::LightmapQuality::Off],
            &[],
            1,
        )
    };
    let mut level = crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"celestial_key","name":"Celestial key","spawn":{"x":0,"z":0}}"#).map_err(|error| error.to_string())?;
    level.global_illuminators.push(source.clone());
    let original_key = stage_key(&level)?;
    level
        .global_illuminators
        .first_mut()
        .ok_or("source")?
        .visual = None;
    assert_eq!(
        original_key,
        stage_key(&level)?,
        "background-only controls must not rebake stored light"
    );
    level
        .global_illuminators
        .first_mut()
        .ok_or("source")?
        .direction = [0.0, -1.0, 0.0];
    assert_ne!(
        original_key,
        stage_key(&level)?,
        "moving the real source still invalidates lighting"
    );
    source.direction = [f32::MAX, -f32::MAX, 1.0];
    source.validate()?;
    assert!(source.source_direction().iter().all(|v| v.is_finite()));
    source.bake = false;
    assert!(
        source.validate().is_err(),
        "a visible source may not silently have no illumination bake"
    );
    assert!(GlobalIlluminatorDef::direction_from_angles(f32::NAN, 30.0).is_err());
    assert!(GlobalIlluminatorDef::direction_from_angles(30.0, 91.0).is_err());
    Ok(())
}

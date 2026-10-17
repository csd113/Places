//! Source-only regressions protect the previous cross-storey fog leak and the
//! single shared GPU budget for newly separate ground layers. These tiny
//! records load no assets, bake no maps and run in well under a millisecond.

use crate::engine::level::{FogRegionDef, GroundFogDef, LevelDef, MAX_FOG_REGIONS};
use crate::engine::loader::validate_level;
use crate::engine::quality::QualityLevel;
use crate::engine::render::{FogRegion, LevelFog};

#[test]
fn elevated_box_and_ground_mist_obey_all_six_bounds() {
    let box_fog = FogRegion::resolve(
        &FogRegionDef {
            id: "upper_box".into(),
            min: [0.0, 3.0, 0.0],
            max: [8.0, 6.0, 8.0],
            density: 0.2,
            color: None,
            falloff_m: Some(1.0),
            ground_y: Some(6.0),
            top_y: Some(6.0),
        },
        [0.5; 3],
    );
    let mist = FogRegion::from_ground(
        &GroundFogDef {
            id: "low_mist".into(),
            min: [0.0, 0.0],
            max: [8.0, 8.0],
            ground_y: 3.0,
            height: 1.0,
            density: 0.2,
            color: None,
            falloff_m: Some(1.0),
        },
        [0.5; 3],
    );
    for volume in [box_fog, mist] {
        for outside in [
            [4.0, 1.5, 4.0], // previous bug: another floor below min Y
            [4.0, 7.0, 4.0],
            [-0.1, 3.0, 4.0],
            [8.1, 3.0, 4.0],
            [4.0, 3.0, -0.1],
            [4.0, 3.0, 8.1],
        ] {
            assert!(
                volume.density_at(outside).abs() < 1.0e-6_f32,
                "all six box faces exclude outside points"
            );
        }
        assert!(
            (volume.density_at([4.0, 3.0, 4.0]) - 0.2).abs() < 1.0e-6,
            "base layer has full density"
        );
        assert!(
            (volume.density_at([0.5, 3.0, 4.0]) - 0.1).abs() < 1.0e-6,
            "horizontal soft edge reaches half density halfway through"
        );
    }
    assert!(
        (box_fog.density_at([4.0, 5.0, 4.0]) - 0.2).abs() < 1.0e-6,
        "full box retains density above ground-mist height"
    );
    assert!(
        (mist.density_at([4.0, 3.5, 4.0]) - 0.1).abs() < 1.0e-6,
        "ground mist fades to half at half height"
    );
    assert!(
        mist.density_at([4.0, 4.0, 4.0]).abs() < 1.0e-6_f32,
        "ground mist vanishes at its top"
    );
}

#[test]
fn box_and_ground_fog_share_validation_identity_and_gpu_budget() -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"fog","name":"Fog",
        "spawn":{"x":1,"z":1},
        "fog_regions":[{"id":"box","min":[0,0,0],"max":[4,3,4],"density":0.1}],
        "ground_fog":[{"id":"ground","min":[0,0],"max":[4,4],"ground_y":0,"height":1,"density":0.2}] }"#,
    ).map_err(|error| error.to_string())?;
    validate_level(&level)?;
    let fog = LevelFog::from_level(&level);
    assert_eq!(
        fog.regions.len(),
        2,
        "both collections resolve into one renderer list"
    );
    assert_eq!(
        fog.uploaded_region_count(QualityLevel::Low),
        2,
        "low quality shares its existing two-volume bound"
    );
    let mut invalid = level.clone();
    invalid
        .ground_fog
        .first_mut()
        .ok_or("missing ground fog")?
        .id = "box".into();
    assert!(
        validate_level(&invalid).is_err(),
        "duplicate identity across collections is ambiguous"
    );
    {
        let ground = invalid.ground_fog.first_mut().ok_or("missing ground fog")?;
        ground.id = "ground".into();
        ground.height = -0.5;
    }
    assert!(
        validate_level(&invalid).is_err(),
        "negative thickness must never silently invert a ground layer"
    );
    {
        let ground = invalid.ground_fog.first_mut().ok_or("missing ground fog")?;
        ground.height = f32::MAX;
        ground.ground_y = f32::MAX;
    }
    assert!(
        validate_level(&invalid).is_err(),
        "finite operands cannot overflow the upper bound"
    );
    let base = level
        .ground_fog
        .first()
        .ok_or("missing ground fog")?
        .clone();
    for index in 1..MAX_FOG_REGIONS {
        let mut next = base.clone();
        next.id = format!("ground_{index}");
        level.ground_fog.push(next);
    }
    assert!(
        validate_level(&level).is_err(),
        "each collection cannot independently claim the full renderer budget"
    );
    drop(level.ground_fog.pop());
    validate_level(&level)?;
    assert_eq!(
        LevelFog::from_level(&level).uploaded_region_count(QualityLevel::High),
        MAX_FOG_REGIONS,
        "the combined maximum fills the existing high quality budget"
    );
    Ok(())
}

#[test]
fn fog_integrates_only_the_sightline_inside_its_box() {
    let region = FogRegion {
        min: [0.0_f32, 3.0_f32, 0.0_f32],
        max: [8.0_f32, 6.0_f32, 8.0_f32],
        density: 0.2_f32,
        color: [0.5_f32; 3],
        falloff_m: 0.0_f32,
        ground_y: 6.0_f32,
        top_y: 6.0_f32,
    };
    let beyond_wall = [4.0_f32, 4.0_f32, 12.0_f32];
    assert!(
        (region.optical_depth([4.0_f32, 4.0_f32, -4.0_f32], beyond_wall) - 1.6_f32).abs()
            < 1.0e-5_f32,
        "a surface beyond the box still receives the eight metres of fog on its sightline"
    );
    assert!(
        (region.optical_depth([4.0_f32, 4.0_f32, 4.0_f32], beyond_wall) - 0.8_f32).abs()
            < 1.0e-5_f32,
        "a camera inside the box only integrates the four metres before exit"
    );
    assert!(
        region
            .optical_depth([4.0_f32, 1.5_f32, -4.0_f32], [4.0_f32, 1.5_f32, 12.0_f32])
            .abs()
            < 1.0e-6_f32,
        "a lower-floor sightline that misses the box stays clear"
    );
    assert!(
        region.optical_depth(beyond_wall, beyond_wall).abs() < 1.0e-6_f32,
        "a zero-length view accumulates no extinction"
    );
    let ground = FogRegion {
        ground_y: 3.0_f32,
        top_y: 4.0_f32,
        max: [8.0_f32, 4.0_f32, 8.0_f32],
        ..region
    };
    assert!(
        (ground.optical_depth([4.0_f32, 3.5_f32, -4.0_f32], [4.0_f32, 3.5_f32, 12.0_f32])
            - 0.8_f32)
            .abs()
            < 1.0e-5_f32,
        "ground mist's half-height sightline integrates half the base extinction"
    );
}

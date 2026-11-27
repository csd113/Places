//! A tiny CPU-only regression: the new wall fixture must produce a real warm
//! direct pool without inheriting the legacy Pool model's paired apertures.
use super::LevelLighting;
use crate::engine::level::LevelDef;
use crate::engine::lighting::{DEFAULT_LIGHT_COLOR, LightColor};

#[test]
fn bulkhead_lights_the_room_without_changing_the_pool_wall_fixture() -> Result<(), String> {
    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"wall_fixtures","name":"Wall fixtures",
        "spawn":{"x":2,"z":2},"rooms":[{"x":0,"z":0,"width":4,"depth":4,"height":2.7}],
        "ceiling_lights":[{"fixture":"maintenance:warm_bulkhead","mount":"wall","x":2,"y":1.7,"z":0}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let [bulkhead] = lighting.lights() else {
        return Err("The bulkhead must retain one source".into());
    };
    assert!(
        bulkhead.z() > 0.151 && !bulkhead.source.caustics,
        "the source should clear the cage with no Pool caustics"
    );
    let pool = lighting.bake_terms(2.0, 1.0, 1.0).pool;
    assert!(
        pool.r > 0.02 && pool.r > pool.g && pool.g > pool.b,
        "the fixture must cast a nonzero warm direct pool, got {pool:?}"
    );
    let authored = level
        .ceiling_lights
        .first_mut()
        .ok_or("missing test fixture")?;
    authored.color = Some(LightColor::WHITE);
    assert_eq!(
        authored.emitted_color(),
        LightColor::WHITE,
        "authoring an explicit RGB colour must override the warm default"
    );
    authored.color = None;
    authored.fixture = "core:pool_light_wall".into();
    let legacy_lighting = LevelLighting::bake(&level);
    let [lower, upper] = legacy_lighting.lights() else {
        return Err("The existing Pool wall fixture lost its two strips".into());
    };
    assert!(
        (upper.y() - lower.y() - 0.09).abs() < 0.0001,
        "legacy strip spacing changed"
    );
    for strip in [lower, upper] {
        assert_eq!(
            strip.source.color, DEFAULT_LIGHT_COLOR,
            "legacy colour changed"
        );
        assert!(strip.source.caustics, "legacy Pool caustics changed");
    }
    Ok(())
}

// A unit-room lighting record only: guard exact non-cardinal aperture yaw
// across package encode/decode and ensure the other wall fixture retains v2.
#[test]
fn bulkhead_wall_aperture_yaw_survives_package_round_trip() -> Result<(), String> {
    use crate::engine::lighting::transport::{EmitterShape, TransportEmitter};
    use crate::engine::package::lighting::{read_lighting, write_lighting};

    let mut level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"wall_yaw","name":"Wall yaw",
        "spawn":{"x":2,"z":2},"rooms":[{"x":0,"z":0,"width":4,"depth":4,"height":2.7}],
        "ceiling_lights":[{"fixture":"maintenance:warm_bulkhead","mount":"wall","x":2,"y":1.7,"z":0.2,
        "rotation_degrees":15}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let encoded = write_lighting(&lighting)?;
    assert_eq!(
        encoded.get(4..6),
        Some(3_u16.to_le_bytes().as_slice()),
        "angled bulkhead needs v3"
    );
    let decoded = read_lighting(&encoded)?;
    assert_eq!(
        write_lighting(&decoded)?,
        encoded,
        "angled record bytes must be stable"
    );
    for record in [&lighting, &decoded] {
        let [light] = record.lights() else {
            return Err("expected exactly one bulkhead emitter".into());
        };
        let EmitterShape::Rect { u, v } = TransportEmitter::from_baked(light, None).shape else {
            return Err("expected a rectangular emitter".into());
        };
        let yaw = 15_f32.to_radians();
        assert!(
            glam::Vec3::from_array(u).abs_diff_eq(
                glam::Vec3::new(0.072 * yaw.cos(), 0.0, -0.072 * yaw.sin()),
                1.0e-6
            ),
            "long tap axis must follow the exact wall yaw"
        );
        assert!(
            glam::Vec3::from_array(v).abs_diff_eq(
                glam::Vec3::new(0.000_1 * yaw.sin(), 0.0, 0.000_1 * yaw.cos()),
                1.0e-6
            ),
            "short tap axis must follow the exact wall yaw"
        );
    }
    let fixture = level.ceiling_lights.first_mut().ok_or("fixture")?;
    fixture.fixture = "core:pool_light_wall".into();
    let stock = write_lighting(&LevelLighting::bake(&level))?;
    assert_eq!(
        stock.get(4..6),
        Some(2_u16.to_le_bytes().as_slice()),
        "stock keeps v2"
    );
    assert_eq!(
        write_lighting(&read_lighting(&stock)?)?,
        stock,
        "stock record bytes must be stable"
    );
    Ok(())
}

// Codec coverage for exact-cardinal axes that the old saturating conversion
// can disagree with. Nearby legacy-compatible axes must still use v2.
#[test]
fn bulkhead_cardinal_yaw_uses_exact_record_only_when_v2_would_change_it() -> Result<(), String> {
    use super::EmitterYaw;
    use crate::engine::package::lighting::{read_lighting, write_lighting};

    let fixture_json = r#"{"schema":"places-level","format_version":1,"id":"wall_cardinal","name":"Wall cardinal",
        "spawn":{"x":2,"z":2},"rooms":[{"x":0,"z":0,"width":4,"depth":4,"height":2.7}],
        "ceiling_lights":[{"fixture":"maintenance:warm_bulkhead","mount":"wall","x":2,"y":1.7,"z":0.2,
        "rotation_degrees":YAW}]}"#;
    for (yaw, axis, version) in [
        (f32::MAX, EmitterYaw::AlongX, 3_u16),
        (-f32::MAX, EmitterYaw::AlongX, 3),
        (-360.0, EmitterYaw::AlongX, 2),
        (450.0, EmitterYaw::AlongZ, 2),
        (-270.0, EmitterYaw::AlongZ, 2),
    ] {
        let mut level = LevelDef::from_json(&fixture_json.replace("YAW", &format!("{yaw:e}")))
            .map_err(|error| error.to_string())?;
        crate::engine::loader::validate_level(&level)?;
        let baked = LevelLighting::bake(&level);
        let [before_light] = baked.lights() else {
            return Err("expected one bulkhead emitter".into());
        };
        assert_eq!(
            before_light.source.rotation_degrees.to_bits(),
            yaw.to_bits(),
            "authored yaw {yaw:e}"
        );
        assert_eq!(before_light.emitter_yaw, axis, "baked axis for yaw {yaw:e}");
        let encoded = write_lighting(&baked)?;
        let decoded = read_lighting(&encoded)?;
        let [after_light] = decoded.lights() else {
            return Err("expected one decoded bulkhead emitter".into());
        };
        assert_eq!(
            after_light.emitter_yaw, axis,
            "decoded axis for yaw {yaw:e}"
        );
        assert_eq!(
            encoded.get(4..6),
            Some(version.to_le_bytes().as_slice()),
            "record version for yaw {yaw:e}"
        );
        assert_eq!(
            write_lighting(&decoded)?,
            encoded,
            "bulkhead bytes for yaw {yaw:e}"
        );

        if version == 3 {
            // A pre-v3 record at this same yaw still reconstructs its
            // historical Z axis. Do not reinterpret archives as exact yaw.
            let mut historical = baked;
            let [historical_light] = historical.lights.as_mut_slice() else {
                return Err("expected one historical wall aperture".into());
            };
            historical_light.emitter_yaw = EmitterYaw::AlongZ;
            for (legacy_attached_yaw, legacy_version) in [(true, 1_u16), (false, 2)] {
                historical.legacy_attached_yaw = legacy_attached_yaw;
                let archived = write_lighting(&historical)?;
                assert_eq!(
                    archived.get(4..6),
                    Some(legacy_version.to_le_bytes().as_slice()),
                    "historical record version for yaw {yaw:e}"
                );
                let archived_light = read_lighting(&archived)?;
                let [historical_aperture] = archived_light.lights() else {
                    return Err("expected one decoded historical wall aperture".into());
                };
                assert_eq!(
                    historical_aperture.emitter_yaw,
                    EmitterYaw::AlongZ,
                    "legacy v{legacy_version} axis for yaw {yaw:e}"
                );
                assert_eq!(
                    write_lighting(&archived_light)?,
                    archived,
                    "legacy v{legacy_version} bytes for yaw {yaw:e}"
                );
            }
        }

        // Stock never changes its legacy axis or emits a v3 record even at
        // the same yaw where a bulkhead would disagree with the old reader.
        level.ceiling_lights.first_mut().ok_or("fixture")?.fixture = "core:pool_light_wall".into();
        let stock = write_lighting(&LevelLighting::bake(&level))?;
        assert_eq!(
            stock.get(4..6),
            Some(2_u16.to_le_bytes().as_slice()),
            "stock version for yaw {yaw:e}"
        );
        let stock_decoded = read_lighting(&stock)?;
        let [lower, upper] = stock_decoded.lights() else {
            return Err("expected the two stock apertures".into());
        };
        let legacy_axis = EmitterYaw::fixture(yaw);
        assert_eq!(
            (lower.emitter_yaw, upper.emitter_yaw),
            (legacy_axis, legacy_axis),
            "stock legacy axes for yaw {yaw:e}"
        );
        assert_eq!(
            write_lighting(&stock_decoded)?,
            stock,
            "stock bytes for yaw {yaw:e}"
        );
    }
    Ok(())
}

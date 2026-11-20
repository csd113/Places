//! One small in-memory room checks Static footprint distances and CPU-side
//! Full emitter samples. These checks need no assets, atlas solve or GPU work.

use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::transport::TransportEmitter;

fn yaw_test_level() -> Result<LevelDef, String> {
    LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"yaw","name":"Yaw",
        "spawn":{"x":1,"z":1},"rooms":[{"width":6,"depth":6,"height":3}],
        "ceiling_lights":[{"x":1,"z":1,"rotation_degrees":45,"fixture":"core:fluorescent_panel_01"}],
        "props":[{"model":"core:chair","x":3,"z":3,"rotation_degrees":30,"lights":[
            {"shape":"line","length":3,"offset":[0,1,0],"rotation_degrees":15},
            {"shape":"rect","half_width":1.5,"half_depth":0.1,"offset":[0,1,0],"rotation_degrees":15},
            {"shape":"line","length":3,"offset":[0,1,0],"rotation_degrees":60}]}]}"#,
    )
    .map_err(|error| error.to_string())
}

#[test]
fn attached_light_yaw_reaches_static_footprints_and_full_cpu_emitters() -> Result<(), String> {
    let level = yaw_test_level()?;
    crate::engine::loader::validate_level(&level)?;
    let bake = LevelLighting::bake(&level);
    let lights = bake.lights();
    let [stock, diagonal, rectangle, right_angle] = lights else {
        return Err("Expected one fixture and three attached sources".into());
    };
    let terms = |x, z| bake.pool_terms_in_room(Some(0), x, 1.0, z);
    let along = terms(3.8, 2.2);
    let across = terms(3.8, 3.8);
    for source in [1, 2] {
        let distance = |row: &[crate::engine::lighting::PoolTerm]| {
            row.iter()
                .find(|term| term.light == source)
                .map(|term| term.horizontal_m)
                .ok_or("Missing Static contribution")
        };
        assert!(
            distance(&along)? < 0.000_01 && distance(&across)? > 1.0,
            "Static must measure from the rotated footprint, including the parent yaw"
        );
    }

    let line = TransportEmitter::from_baked(diagonal, None)
        .shape
        .samples(2);
    let [start, end] = line.as_slice() else {
        return Err("Expected two Full line taps".into());
    };
    assert!(
        start[0] < -1.0 && start[2] > 1.0 && end[0] > 1.0 && end[2] < -1.0,
        "Full line taps must rotate into the same diagonal as the Static footprint"
    );
    let area = TransportEmitter::from_baked(rectangle, None)
        .shape
        .samples(2);
    let [corner, across_short, across_long, _opposite] = area.as_slice() else {
        return Err("Expected a two-by-two CPU rectangle sample grid".into());
    };
    let origin_tap = glam::Vec3::from_array(*corner);
    let long_edge = glam::Vec3::from_array(*across_long) - origin_tap;
    let short_edge = glam::Vec3::from_array(*across_short) - origin_tap;
    let diagonal_component = std::f32::consts::FRAC_1_SQRT_2;
    assert!(
        long_edge.abs_diff_eq(
            glam::Vec3::new(diagonal_component, 0.0, -diagonal_component) * 1.5,
            1.0e-5
        ),
        "CPU rectangle sample grid must span the rotated long axis: {long_edge:?}"
    );
    assert!(
        short_edge.abs_diff_eq(
            glam::Vec3::new(diagonal_component, 0.0, diagonal_component) * 0.1,
            1.0e-5
        ),
        "CPU rectangle sample grid must span the perpendicular short axis: {short_edge:?}"
    );
    assert_eq!(
        TransportEmitter::from_baked(right_angle, None)
            .shape
            .samples(2),
        vec![[0.0, 0.0, -1.5], [0.0, 0.0, 1.5]],
        "A 90-degree line keeps the original order and positions"
    );
    assert!(
        stock.half_d() > stock.half_w(),
        "Stock housing and aperture keep their mounting rule"
    );
    Ok(())
}

#[test]
fn attached_light_composes_extreme_yaw_and_keeps_invalid_local_fallback() -> Result<(), String> {
    let mut level = yaw_test_level()?;
    // This finite value is 120 degrees modulo a full turn. Doubling it
    // overflows f32, but the symmetric line should still resolve to 60 degrees.
    let extreme = f32::from_bits(0x7f7f_fff0);
    let cases = [
        (extreme, extreme, [0.75, 0.0, -1.299_038]),
        (-extreme, -extreme, [0.75, 0.0, 1.299_038]),
        (f32::MAX, 90.0, [0.0, 0.0, 1.5]),
        (30.0, f32::NAN, [1.5, 0.0, 0.0]),
    ];
    for (parent_yaw, local_yaw, expected_end) in cases {
        let [prop] = level.props.as_mut_slice() else {
            return Err("Expected one attached-light prop".into());
        };
        let [line, ..] = prop.lights.as_mut_slice() else {
            return Err("Expected an attached line".into());
        };
        prop.rotation_degrees = parent_yaw;
        line.rotation_degrees = local_yaw;
        let validation = crate::engine::loader::validate_level(&level);
        if local_yaw.is_finite() {
            validation?;
        } else {
            // Direct callers can still hand the bake an unvalidated level.
            assert!(validation.is_err(), "The loader must reject non-finite yaw");
        }

        let bake = LevelLighting::bake(&level);
        let [_, light, ..] = bake.lights() else {
            return Err("Expected a fixture followed by an attached line".into());
        };
        let taps = TransportEmitter::from_baked(light, None).shape.samples(2);
        let [_, end] = taps.as_slice() else {
            return Err("Expected two CPU line taps".into());
        };
        assert!(
            glam::Vec3::from_array(*end).abs_diff_eq(glam::Vec3::from_array(expected_end), 1.0e-5),
            "Yaw {parent_yaw} + {local_yaw} must place the end tap at {expected_end:?}, got {end:?}"
        );
    }
    Ok(())
}

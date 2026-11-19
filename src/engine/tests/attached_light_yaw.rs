//! One small in-memory room guards the Static nearest point and Full source
//! axes against the stock fixture snap. No assets, atlas solve or GPU work.

use crate::engine::level::LevelDef;
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::transport::TransportEmitter;

#[test]
fn attached_light_yaw_reaches_both_bakers() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"yaw","name":"Yaw",
        "spawn":{"x":1,"z":1},"rooms":[{"width":6,"depth":6,"height":3}],
        "ceiling_lights":[{"x":1,"z":1,"rotation_degrees":45,"fixture":"core:fluorescent_panel_01"}],
        "props":[{"model":"core:chair","x":3,"z":3,"rotation_degrees":30,"lights":[
            {"shape":"line","length":3,"offset":[0,1,0],"rotation_degrees":15},
            {"shape":"rect","half_width":1.5,"half_depth":0.1,"offset":[0,1,0],"rotation_degrees":15},
            {"shape":"line","length":3,"offset":[0,1,0],"rotation_degrees":60}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
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
    assert!(
        area.iter()
            .all(|tap| (tap[0] + tap[2]).abs() < 0.1 && tap[0].abs() > 0.4),
        "Full area taps must span the authored long axis and short axis"
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

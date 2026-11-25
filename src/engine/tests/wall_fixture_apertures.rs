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

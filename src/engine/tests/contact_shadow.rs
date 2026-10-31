//! One source-only regression for the buried runtime-crate footprint. No
//! existing test covers contact admission; two tiny rooms need no assets/bake/GPU.
use super::contact_shadow;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::spatial::Aabb;

#[test]
fn buried_contact_rejects_overhead_floors_without_losing_supported_casters() -> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"contact","name":"Contact",
        "spawn":{"x":0,"z":0},"rooms":[
        {"x":-2,"z":-2,"width":4,"depth":4,"floor_y":3},
        {"x":-2,"z":-2,"width":4,"depth":4,"floor_y":0}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let surfaces = LevelSurfaces::new(&level);
    let shadow = |bottom, top| {
        contact_shadow(
            Aabb {
                min: [-0.3, bottom, -0.3],
                max: [0.3, top, 0.3],
            },
            &surfaces,
            [0.0, 1.0, 0.0],
        )
    };
    assert!(
        shadow(-2.0, -1.35).is_none(),
        "buried crate cannot project up through its floor"
    );
    assert!(
        shadow(-0.65, 0.0).is_none(),
        "a top flush with the floor remains wholly buried"
    );
    let lower = shadow(0.0, 0.65).ok_or("lost lower-storey resting contact")?;
    let upper = shadow(3.0, 3.65).ok_or("lost upper-storey resting contact")?;
    assert!(
        lower.position_floor[3].abs() < f32::EPSILON,
        "upper room order must not steal lower support"
    );
    assert!(
        (upper.position_floor[3] - 3.0).abs() < f32::EPSILON,
        "upper storey keeps its floor"
    );
    let held = shadow(0.25, 0.9).ok_or("lost low held/suspended contact")?;
    assert!(
        (held.incoming[3] - 0.5).abs() < f32::EPSILON,
        "existing suspended attenuation survives"
    );
    assert!(
        shadow(0.6, 1.25).is_none(),
        "high held objects retain distance fade-out"
    );
    assert!(
        shadow(-0.1, 0.55).is_some(),
        "partly embedded visible casters remain eligible"
    );
    Ok(())
}

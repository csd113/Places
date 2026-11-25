//! CPU-only fit checks, independent of packaged art and GPU.
use super::{LevelDef, LevelSurfaces, Vec3, WalkableFloor, conduit_stock};

#[test]
fn electrical_panel_conduits_fit_correct_storey_and_skip_open_roofs() -> Result<(), String> {
    let mut level = LevelDef::from_json(r#"{
        "schema":"places-level","format_version":1,"id":"conduits","name":"Conduits",
        "spawn":{"x":1,"z":1},
        "rooms":[{"width":4,"depth":4,"height":2.6},{"width":4,"depth":4,"floor_y":3,"height":4}],
        "walls":[{"x":0,"y":0,"z":0.8,"width":4,"depth":0.198,"height":2.6},{"x":0,"y":3,"z":0.8,"width":4,"depth":0.198,"height":4}],
        "props":[{"model":"maintenance:electrical_panel","solid":true,"electrical_panel":{},"components":[{"component":"interactable"}],"x":1,"y":1.5,"z":1},{"model":"maintenance:electrical_panel","solid":true,"electrical_panel":{},"components":[{"component":"interactable"}],"x":1,"y":4.5,"z":1}]
    }"#).map_err(|error| error.to_string())?;
    let heights = |scene: &LevelDef| {
        let floor = WalkableFloor::from_level(scene);
        let surfaces = LevelSurfaces::new(scene);
        scene
            .props
            .iter()
            .map(|prop| {
                conduit_stock(scene, &floor, &surfaces, prop)
                    .first()
                    .map(|(_, fit)| {
                        [
                            fit.transform_point3(Vec3::ZERO).y,
                            fit.transform_point3(Vec3::Y).y,
                        ]
                    })
            })
            .collect::<Vec<_>>()
    };
    let actual = heights(&level);
    for (point, expected) in actual.iter().zip([[1.86_f32, 2.6], [4.86, 7.0]]) {
        let [bottom, top] = point.ok_or("missing fit")?;
        assert!(
            (bottom - expected[0]).abs() < 0.001 && (top - expected[1]).abs() < 0.001,
            "riser meets the cabinet and the correct ceiling at each height"
        );
    }
    level.rooms.get_mut(1).ok_or("upper room")?.ceiling =
        crate::engine::level::CeilingProfileDef::Open;
    assert!(
        heights(&level).last().is_some_and(Option::is_none),
        "open room never gets stretched pipes"
    );
    level.walls.first_mut().ok_or("backing wall")?.height = Some(2.0);
    assert!(
        heights(&level).first().is_some_and(Option::is_none),
        "a real ceiling above an unbacked run cannot grow floating conduits"
    );
    crate::engine::electrical_panel::validate(&level)?;
    level.props.first_mut().ok_or("cabinet")?.z = 1.05;
    assert!(
        crate::engine::electrical_panel::validate(&level).is_err(),
        "unsupported saved cabinet is rejected"
    );
    assert!(
        heights(&level).first().is_some_and(Option::is_none),
        "unsupported source also cannot emit conduit geometry"
    );
    Ok(())
}

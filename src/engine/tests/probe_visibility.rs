//! Essential regression for hoisted probe guards: corrupt labels, points below
//! floors and stacked-room ownership must not become valid light sources. Three
//! tiny rooms and one doorway compare against the original scalar predicate;
//! no assets, atlas solve, native window or GPU. Runtime is milliseconds.
use super::LevelLighting;
use crate::engine::level::LevelDef;

fn original_predicate(
    lighting: &LevelLighting,
    position: [f32; 3],
    probe: [f32; 3],
    label: i32,
) -> bool {
    let area = |point: [f32; 3]| {
        let room = lighting.room_index_at_height(point[0], point[1], point[2])?;
        lighting.probe_region_at(room, point)
    };
    usize::try_from(label).ok() == lighting.room_index_at_height(probe[0], probe[1], probe[2])
        && area(position).is_some()
        && area(probe).is_some()
        && lighting.runtime_probe_segment_visible(position, probe)
}

#[test]
fn prepared_probe_receiver_preserves_labels_air_volumes_and_doorway_visibility()
-> Result<(), String> {
    let level = LevelDef::from_json(
        r#"{"schema":"places-level","format_version":1,"id":"probe_guards","name":"Probe guards",
        "spawn":{"x":0,"z":0},
        "rooms":[{"x":-2,"z":-2,"width":4,"depth":4,"floor_y":0,"height":2.5},
                 {"x":-2,"z":-2,"width":4,"depth":4,"floor_y":3,"height":2.5},
                 {"x":2,"z":-2,"width":4,"depth":4,"floor_y":0,"height":2.5}],
        "walls":[{"x":1.9,"z":-2,"width":0.2,"depth":4,"height":2.5,
            "openings":[{"kind":"passage","offset":1.4,"width":1.2,"height":2.2}]}]}"#,
    )
    .map_err(|error| error.to_string())?;
    let lighting = LevelLighting::bake(&level);
    let positions = [
        [0.0, 1.2, 0.0],
        [0.0, 3.8, 0.0],
        [0.0, -0.5, 0.0],
        [0.0, 2.75, 0.0],
    ];
    let probes = [
        [0.5, 1.2, 0.0],
        [0.5, 3.8, 0.0],
        [4.0, 1.2, 0.0],
        [4.0, 1.2, 1.5],
        [0.0, -0.5, 0.0],
    ];
    for position in positions {
        let visible = lighting.labelled_probe_visibility(position);
        for probe in probes {
            for label in [-1_i32, 0_i32, 1_i32, 2_i32, i32::MAX] {
                assert_eq!(
                    visible(probe, label),
                    original_predicate(&lighting, position, probe, label),
                    "prepared predicate changed receiver {position:?}, probe {probe:?}, label {label}"
                );
            }
        }
    }
    let ground = lighting.labelled_probe_visibility(positions[0]);
    assert!(
        ground(probes[2], 2_i32),
        "an open doorway keeps valid cross-room support"
    );
    assert!(
        !ground(probes[3], 2_i32),
        "the solid wall beside the doorway still blocks"
    );
    assert!(
        !ground(probes[0], 1_i32),
        "a forged stacked-room owner stays refused"
    );
    assert!(
        !ground(probes[4], 0_i32),
        "XZ fallback cannot admit a probe below the floor"
    );
    Ok(())
}

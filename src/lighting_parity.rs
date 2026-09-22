//! Rust/JavaScript parity vectors for the static lighting model.
//!
//! `src/lighting.rs` is authoritative; `level-editor/js/lighting.js` mirrors it
//! for the editor's 3D preview. These tests lock a set of representative
//! scenarios to the Rust values and the editor test
//! (`level-editor/tests/lighting-parity.test.mjs`) replays the exact same file
//! inside a tolerance, so drift between the two implementations fails loudly.
//!
//! Baked illumination is three-channel RGB, so vectors carry `[r, g, b]`
//! arrays: an editor preview that mirrored only brightness would no longer be
//! able to show the coloured light the game bakes.
//!
//! The vector file is generated once and checked in. Regenerate it after an
//! intentional lighting change with:
//!
//! ```text
//! cargo test -- --ignored generate_lighting_parity_vectors --nocapture
//! ```
//!
//! then run both suites. The Rust test below fails if the file no longer
//! matches the code, which is the point: the numbers cannot drift silently.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout
)]
#![cfg(test)]

use crate::level::LevelDef;
use crate::lighting::{LevelLighting, LightColor};

/// One parity scenario: raw level data plus the room samples to compare.
struct Scenario {
    name: &'static str,
    rooms: &'static str,
    walls: &'static str,
    lights: &'static str,
    /// (room index, x, y, z) samples, one per line in the vector file.
    samples: &'static [(usize, f32, f32, f32)],
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "normal_room_16m2_two_lights",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.5 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 1.4, "z": 1.4 },
                 { "fixture": "core:fluorescent_panel_01", "x": 2.6, "z": 2.6 }"#,
        samples: &[(0, 2.0, 0.0, 2.0), (0, 0.2, 0.0, 0.2), (0, 3.8, 3.0, 3.8)],
    },
    Scenario {
        name: "large_underlit_room",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 18.0, "z": 18.0 }"#,
        samples: &[
            (0, 10.0, 0.0, 10.0),
            (0, 2.0, 0.0, 2.0),
            (0, 1.0, 0.0, 19.0),
        ],
    },
    Scenario {
        name: "level1_sparse_grid",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 52.0, "depth": 54.0, "height": 3.5 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 13.5, "z": 13.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 13.5, "z": 26.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 13.5, "z": 39.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 27.0, "z": 13.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 27.0, "z": 26.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 27.0, "z": 39.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 40.5, "z": 13.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 40.5, "z": 26.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 40.5, "z": 39.0 }"#,
        samples: &[
            (0, 27.0, 0.0, 26.0),
            (0, 13.5, 0.0, 13.0),
            (0, 2.0, 0.0, 2.0),
            (0, 50.0, 0.0, 50.0),
        ],
    },
    Scenario {
        name: "dense_bright_room",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 2.6 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 1.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 1.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 3.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 3.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 }"#,
        samples: &[(0, 2.0, 0.0, 2.0), (0, 0.1, 0.0, 0.1)],
    },
    Scenario {
        name: "tall_room",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 8.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0 }"#,
        samples: &[(0, 4.0, 0.0, 4.0), (0, 4.0, 7.9, 4.0)],
    },
    Scenario {
        name: "weak_fixture",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0, "intensity": 0.5 }"#,
        samples: &[(0, 4.0, 0.0, 4.0), (0, 7.0, 0.0, 4.0)],
    },
    Scenario {
        name: "strong_fixture",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0, "brightness": 2.0 }"#,
        samples: &[(0, 4.0, 0.0, 4.0), (0, 7.0, 0.0, 4.0)],
    },
    Scenario {
        name: "doorway_blend",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
                 { "x": 10.4, "z": 0.0, "width": 20.0, "depth": 10.0, "height": 3.0 }"#,
        walls: r#"{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                    "openings": [{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }] }"#,
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 8.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 8.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0 },
                 { "fixture": "core:fluorescent_panel_01", "x": 20.0, "z": 5.0 }"#,
        samples: &[
            (0, 9.95, 0.0, 5.0),
            (1, 10.45, 0.0, 5.0),
            (0, 1.0, 0.0, 1.0),
            (1, 25.0, 0.0, 5.0),
        ],
    },
    Scenario {
        name: "local_fixture_falloff",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 8.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }"#,
        samples: &[
            (0, 4.0, 0.0, 4.0),
            (0, 5.0, 0.0, 4.0),
            (0, 7.0, 0.0, 4.0),
            (0, 10.0, 0.0, 4.0),
            (0, 20.0, 0.0, 4.0),
        ],
    },
    Scenario {
        name: "red_and_blue_room",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 16.0, "depth": 8.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0,
                     "color": [1.0, 0.0, 0.0] },
                 { "fixture": "core:fluorescent_panel_01", "x": 12.0, "z": 4.0,
                     "color": [0.0, 0.0, 1.0] }"#,
        samples: &[(0, 4.0, 0.0, 4.0), (0, 8.0, 0.0, 4.0), (0, 12.0, 0.0, 4.0)],
    },
    Scenario {
        name: "warm_cool_three_colour",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 18.0, "depth": 6.0, "height": 3.0 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0,
                     "color": [1.0, 0.55, 0.1] },
                 { "fixture": "core:fluorescent_panel_01", "x": 9.0, "z": 3.0,
                     "color": [0.2, 0.4, 1.0] },
                 { "fixture": "core:fluorescent_panel_01", "x": 15.0, "z": 3.0,
                     "color": [0.1, 1.0, 0.3] }"#,
        samples: &[
            (0, 3.0, 0.0, 3.0),
            (0, 9.0, 0.0, 3.0),
            (0, 15.0, 0.0, 3.0),
            (0, 6.0, 1.6, 3.0),
        ],
    },
    Scenario {
        name: "coloured_tall_room",
        rooms: r#"{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 6.5 }"#,
        walls: "",
        lights: r#"{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0,
                     "color": [0.9, 0.3, 0.1], "intensity": 1.5 }"#,
        samples: &[(0, 5.0, 0.0, 5.0), (0, 5.0, 6.4, 5.0)],
    },
];

/// Full level JSON for one scenario: the same bytes both implementations read.
fn scenario_level_json(index: usize) -> String {
    let scenario = &SCENARIOS[index];
    format!(
        r#"{{
            "format_version": 1,
            "id": "parity_{index}",
            "name": "Parity {index}",
            "spawn": {{ "x": 0.0, "z": 0.0 }},
            "rooms": [{}],
            "walls": [{}],
            "ceiling_lights": [{}]
        }}"#,
        scenario.rooms, scenario.walls, scenario.lights
    )
}

/// `[r, g, b]` JSON array for one colour.
fn color_json(color: LightColor) -> serde_json::Value {
    serde_json::json!([color.r, color.g, color.b])
}

/// Builds the complete vector document from the current Rust implementation.
fn build_vectors_document() -> serde_json::Value {
    let mut scenarios = Vec::new();
    for (index, scenario) in SCENARIOS.iter().enumerate() {
        let json = scenario_level_json(index);
        let level = LevelDef::from_json(&json).expect("parity scenario parses");
        let lighting = LevelLighting::bake(&level);
        let rooms: Vec<serde_json::Value> = lighting
            .rooms()
            .iter()
            .map(|room| {
                serde_json::json!({
                    "area": room.area_m2,
                    "fixture_count": room.fixture_count,
                    "effective_power": color_json(room.effective_power),
                    "baseline": color_json(room.baseline),
                })
            })
            .collect();
        let samples: Vec<serde_json::Value> = scenario
            .samples
            .iter()
            .map(|(room, x, y, z)| {
                serde_json::json!({
                    "room": room,
                    "x": x,
                    "y": y,
                    "z": z,
                    "value": color_json(lighting.sample_in_room(*room, *x, *y, *z)),
                })
            })
            .collect();
        scenarios.push(serde_json::json!({
            "name": scenario.name,
            "level": serde_json::from_str::<serde_json::Value>(&json).expect("valid json"),
            "expect": { "rooms": rooms, "samples": samples },
        }));
    }
    serde_json::json!({
        "comment": "Generated by src/lighting_parity.rs; Rust is authoritative. Values are [r, g, b]. Regenerate with: cargo test -- --ignored generate_lighting_parity_vectors",
        "scenarios": scenarios,
    })
}

/// Regenerates `level-editor/tests/support/lighting_vectors.json`.
///
/// Ignored by default: run it only for an intentional lighting change, then run
/// the Rust and editor parity tests before committing the new file.
#[test]
#[ignore = "writes the checked-in parity vector file"]
fn generate_lighting_parity_vectors() {
    let document = build_vectors_document();
    let path = "level-editor/tests/support/lighting_vectors.json";
    let pretty = serde_json::to_string_pretty(&document).expect("serialize vectors");
    std::fs::write(path, format!("{pretty}\n")).unwrap_or_else(|error| {
        panic!("cannot write {path}: {error}; run from the app directory");
    });
    println!("wrote {path}");
}

/// Reads one `[r, g, b]` array from the vector document.
fn read_color(value: &serde_json::Value, context: &str) -> [f32; 3] {
    let array = value
        .as_array()
        .unwrap_or_else(|| panic!("{context}: expected an [r, g, b] array"));
    assert_eq!(array.len(), 3, "{context}: expected three channels");
    [
        array[0].as_f64().unwrap_or_else(|| panic!("{context}: r")) as f32,
        array[1].as_f64().unwrap_or_else(|| panic!("{context}: g")) as f32,
        array[2].as_f64().unwrap_or_else(|| panic!("{context}: b")) as f32,
    ]
}

/// The checked-in vectors must match the current Rust implementation.
#[test]
fn lighting_parity_vectors_match_rust() {
    let path = "level-editor/tests/support/lighting_vectors.json";
    let text = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("{path} must be checked in and readable: {error}");
    });
    let document: serde_json::Value = serde_json::from_str(&text).expect("vectors parse");
    let scenarios = document["scenarios"].as_array().expect("scenarios array");
    assert_eq!(
        scenarios.len(),
        SCENARIOS.len(),
        "the vector file has a different scenario count; regenerate it"
    );

    for (index, scenario) in SCENARIOS.iter().enumerate() {
        let entry = &scenarios[index];
        assert_eq!(entry["name"].as_str(), Some(scenario.name));
        let expected = &entry["expect"];
        let level: LevelDef =
            serde_json::from_value(entry["level"].clone()).expect("scenario level parses");
        let lighting = LevelLighting::bake(&level);

        let expected_rooms = expected["rooms"].as_array().expect("rooms array");
        assert_eq!(expected_rooms.len(), lighting.rooms().len());
        for (room, expected) in lighting.rooms().iter().zip(expected_rooms) {
            let context = format!("{}: room", scenario.name);
            for (field, actual) in [
                ("area", room.area_m2),
                ("fixture_count", room.fixture_count as f32),
            ] {
                let wanted = expected[field]
                    .as_f64()
                    .unwrap_or_else(|| panic!("{}: missing {field}", scenario.name))
                    as f32;
                assert!(
                    (actual - wanted).abs() < 1e-5,
                    "{}: room {field} drifted: {actual} vs vector {wanted}",
                    scenario.name
                );
            }
            for (field, actual) in [
                ("effective_power", room.effective_power),
                ("baseline", room.baseline),
            ] {
                let wanted = read_color(&expected[field], &format!("{context} {field}"));
                for (channel, (actual, wanted)) in actual.to_array().iter().zip(wanted).enumerate()
                {
                    assert!(
                        (actual - wanted).abs() < 1e-5,
                        "{}: room {field}[{channel}] drifted: {actual} vs vector {wanted}",
                        scenario.name
                    );
                }
            }
        }

        let expected_samples = expected["samples"].as_array().expect("samples array");
        assert_eq!(expected_samples.len(), scenario.samples.len());
        for (sample, expected) in scenario.samples.iter().zip(expected_samples) {
            let (room, x, y, z) = *sample;
            let actual = lighting.sample_in_room(room, x, y, z);
            let wanted = read_color(
                &expected["value"],
                &format!("{}: sample ({room}, {x}, {y}, {z})", scenario.name),
            );
            for (channel, (actual, wanted)) in actual.to_array().iter().zip(wanted).enumerate() {
                assert!(
                    (actual - wanted).abs() < 1e-5,
                    "{}: sample ({room}, {x}, {y}, {z})[{channel}] drifted: {actual} vs vector {wanted}",
                    scenario.name
                );
            }
        }
    }
}

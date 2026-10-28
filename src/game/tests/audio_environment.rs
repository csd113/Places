//! A tiny CPU-only locality control protects elevated and rotated duct anchors.
//! It also verifies that duplicate stock shares one candidate and rebuilding an
//! empty scene drops old anchors. No models, audio devices or bakes are needed.
use glam::Vec3;

use super::{duct_sources, nearest};
use crate::engine::level::LevelDef;
use crate::engine::loader::PropCatalog;

#[test]
fn duct_locality_preserves_rotations_floor_height_and_empty_scene_rebuild() -> Result<(), String> {
    let catalog_json = serde_json::json!({
        "format_version":2_u32,
        "assets":[{
            "id":"core:audio_duct_control","display_name":"Duct audio control",
            "asset_class":"core","asset_type":"prop","source":"file",
            "model":"shared/props/audio_control.glb","size":[0.6_f32,0.6_f32,2.0_f32],
            "tags":["hvac-duct"]
        }]
    });
    let catalog = PropCatalog::from_json_str(&catalog_json.to_string())?;
    let mut level = LevelDef::from_json(
        r#"{
        "schema":"places-level","format_version":1,"id":"duct_audio","name":"Duct audio",
        "spawn":{"x":1,"z":1},
        "rooms":[{"x":0,"z":0,"width":16,"depth":16,"floor_y":4,"height":12}],
        "props":[{"model":"core:audio_duct_control","x":5,"y":2,"z":6}]
    }"#,
    )
    .map_err(|error| error.to_string())?;
    for (yaw, pitch, listener, expected) in rotation_cases() {
        let prop = level.props.first_mut().ok_or("Missing control prop")?;
        // A deliberately oversized collision envelope must not move the
        // acoustic surface away from the actual catalog model.
        prop.size = Some([100.0, 100.0, 100.0]);
        prop.rotation_degrees = yaw;
        prop.pitch_degrees = pitch;
        let sources = duct_sources(&level, &catalog);
        let point = nearest(
            listener,
            sources.iter().map(|source| source.nearest(listener)),
        )
        .ok_or("Missing duct anchor")?;
        assert!(
            point.distance(expected) < 0.0001,
            "Duct audio anchor lost its authored transform"
        );
    }
    let prop = level
        .props
        .first()
        .ok_or("Missing duplicate control")?
        .clone();
    level.props = vec![prop; 100];
    let sources = duct_sources(&level, &catalog);
    assert_eq!(sources.len(), 100, "Every placed envelope must be eligible");
    assert!(
        nearest(
            Vec3::ZERO,
            sources.iter().map(|source| source.nearest(Vec3::ZERO))
        )
        .is_some(),
        "Duplicate stock must resolve one nearest candidate"
    );
    let mut ordinary_catalog = catalog_json;
    ordinary_catalog
        .pointer_mut("/assets/0/tags")
        .ok_or("Missing tags")?
        .clone_from(&serde_json::json!([]));
    assert!(
        duct_sources(
            &level,
            &PropCatalog::from_json_str(&ordinary_catalog.to_string())?
        )
        .is_empty(),
        "Ordinary utility props must not become HVAC sources"
    );
    level.props.clear();
    assert!(
        duct_sources(&level, &catalog).is_empty(),
        "Empty world rebuild must remove previous anchors"
    );
    Ok(())
}

fn rotation_cases() -> [(f32, f32, Vec3, Vec3); 6] {
    [
        (
            0.0_f32,
            0.0_f32,
            Vec3::new(5.0, 6.3, 8.0),
            Vec3::new(5.0, 6.3, 7.0),
        ),
        (
            180.0,
            0.0,
            Vec3::new(5.0, 6.3, 4.0),
            Vec3::new(5.0, 6.3, 5.0),
        ),
        (
            90.0,
            0.0,
            Vec3::new(7.0, 6.3, 6.0),
            Vec3::new(6.0, 6.3, 6.0),
        ),
        (
            270.0,
            0.0,
            Vec3::new(3.0, 6.3, 6.0),
            Vec3::new(4.0, 6.3, 6.0),
        ),
        (
            0.0,
            90.0,
            Vec3::new(5.0, 8.0, 6.3),
            Vec3::new(5.0, 7.0, 6.3),
        ),
        (
            0.0,
            -90.0,
            Vec3::new(5.0, 8.0, 5.7),
            Vec3::new(5.0, 7.0, 5.7),
        ),
    ]
}

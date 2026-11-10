//! Essential regressions for persisted player tint and full head-relative vision.
//! Tiny JSON/matrix fixtures, no model loading, GPU, map compilation or sleeps.
use super::DeathView;
use crate::game::{player_colour::PlayerColour, settings::Settings};
use glam::{Mat4, Quat, Vec3};

// Protects old preferences and exact sRGB selection: a bad/new colour must not
// reset unrelated controls/audio, and a selection must survive serialization.
#[test]
fn player_colour_preserves_legacy_preferences_and_roundtrips() -> Result<(), String> {
    let mut settings: Settings =
        serde_json::from_str(r#"{"master_volume":0.25,"window_width":800}"#)
            .map_err(|error| error.to_string())?;
    assert_eq!(
        settings.player_colour.hex(),
        "#4F7774",
        "default is the requested muted teal"
    );
    let selected = settings.player_colour.step(1);
    settings.player_colour = selected;
    let encoded = serde_json::to_string(&settings).map_err(|error| error.to_string())?;
    let restored: Settings = serde_json::from_str(&encoded).map_err(|error| error.to_string())?;
    assert_eq!(
        restored.player_colour, selected,
        "selection survives save/load"
    );
    assert_eq!(
        restored.window_width, 800,
        "appearance does not change display settings"
    );
    let invalid: Settings =
        serde_json::from_str(r#"{"player_colour":"invalid","master_volume":0.25}"#)
            .map_err(|error| error.to_string())?;
    assert_eq!(
        invalid.player_colour,
        PlayerColour::default(),
        "bad colour alone falls back safely"
    );
    assert!(
        (invalid.master_volume - 0.25).abs() < f32::EPSILON,
        "bad colour cannot discard audio preferences"
    );
    let custom = PlayerColour::parse("#1234aB").ok_or("custom colour rejected")?;
    assert_eq!(
        custom.hex(),
        "#1234AB",
        "custom sRGB values retain exact bytes"
    );
    assert!(
        custom
            .linear_rgb()
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel)),
        "tints reach the renderer in linear colour space"
    );
    Ok(())
}

// Protects a previously unsupported orientation: death must preserve the entry
// eye then follow translation and roll, instead of an upright orbit camera.
#[test]
fn death_camera_preserves_entry_eye_and_physical_head_roll() -> Result<(), String> {
    let head = Mat4::from_translation(Vec3::new(0.0, 1.4, 0.0));
    let eye = Vec3::new(0.0, 1.6, 0.08);
    let view = DeathView::capture(head, eye, Vec3::NEG_Z)?;
    let initial = view.camera(head, 60.0);
    assert!(
        initial.position.distance(eye) < 1.0e-5,
        "entry does not snap to a different eye"
    );
    let fallen = Mat4::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(2.0, 0.35, 1.0),
    );
    let camera = view.camera(fallen, 60.0);
    assert!(
        camera.position.distance(Vec3::new(1.8, 0.35, 1.08)) < 1.0e-5,
        "eye stays attached to the actual head"
    );
    assert!(
        camera.up.distance(Vec3::NEG_X) < 1.0e-5,
        "head roll survives camera construction"
    );
    let (projection, _) =
        camera.view_projection(crate::engine::render::DrawableSize::new(640, 360));
    assert!(
        projection.is_finite(),
        "a sideways physical view has a finite projection"
    );
    assert_eq!(
        camera,
        view.camera(fallen, 60.0),
        "a paused head cannot drift the camera"
    );
    assert!(
        DeathView::capture(Mat4::ZERO, eye, Vec3::NEG_Z).is_err(),
        "invalid skeleton matrices cannot poison a view"
    );
    Ok(())
}

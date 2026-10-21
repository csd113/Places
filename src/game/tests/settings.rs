//! Two focused regressions for settings data loss and native key ownership.
//! Tiny JSON and temporary files need no assets, compiled maps or native window.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::engine::input::canonical_key_name;
use crate::engine::quality::{LightmapQuality, QualityLevel, ReflectionQuality};
use crate::game::settings::{KeyBindings, Settings};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct SettingsDirectory(PathBuf);

impl SettingsDirectory {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "places-settings-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for SettingsDirectory {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn partial_settings_and_failed_saves_preserve_player_choices()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = SettingsDirectory::new()?;
    let path = directory.0.join("settings.json");
    fs::write(
        &path,
        r#"{"bindings":{"forward":"Q"},"quality":"low","lightmaps":"full","reflections":"medium","texture_filtering":"high"}"#,
    )?;
    let settings = Settings::load_or_default_from_path(&path);
    assert_eq!(
        settings.bindings.forward, "Q",
        "partial bindings retain custom keys"
    );
    assert_eq!(
        settings.bindings.console, "`",
        "new controls receive defaults"
    );
    assert_eq!(
        settings.quality_level(),
        QualityLevel::Low,
        "partial settings retain quality"
    );
    assert_eq!(
        settings.lightmap_quality(),
        LightmapQuality::Full,
        "lighting is independent of textures"
    );
    assert_eq!(
        settings.reflection_quality(),
        ReflectionQuality::Medium,
        "explicit reflections remain selected"
    );
    assert_eq!(
        settings.texture_filtering_preset(),
        "high",
        "explicit filtering remains selected"
    );
    settings.save_to_path(&path)?;
    assert_eq!(
        Settings::load_or_default_from_path(&path),
        settings,
        "save/reload preserves every selected value"
    );

    // Exhausted staging names must refuse publication without truncating the
    // previous settings or removing an interrupted file the save does not own.
    let saved = fs::read(&path)?;
    for attempt in 0_u8..16 {
        fs::write(
            directory.0.join(format!("settings.json.partial-{attempt}")),
            b"interrupted",
        )?;
    }
    assert!(
        Settings::default().save_to_path(&path).is_err(),
        "exhausted staging must refuse save"
    );
    assert_eq!(
        fs::read(&path)?,
        saved,
        "failed save must preserve the previous file"
    );
    assert_eq!(
        fs::read(directory.0.join("settings.json.partial-0"))?,
        b"interrupted",
        "failed save must preserve staging artifacts it does not own"
    );

    fs::write(&path, b"invalid first")?;
    let _first_defaults = Settings::load_or_default_reporting(&path);
    fs::write(&path, b"invalid second")?;
    let _second_defaults = Settings::load_or_default_reporting(&path);
    assert_eq!(
        fs::read(path.with_extension("json.invalid"))?,
        b"invalid first",
        "later malformed files must not replace the first backup"
    );
    assert_eq!(
        fs::read(path.with_extension("json.invalid-1"))?,
        b"invalid second",
        "the second malformed file receives a separate backup"
    );
    Ok(())
}

#[test]
fn repaired_bindings_preserve_valid_owners_and_native_names()
-> Result<(), Box<dyn std::error::Error>> {
    let mut bindings = KeyBindings {
        forward: "not a native key".to_string(),
        backward: " w ".to_string(),
        strafe_left: "KP_MINUS".to_string(),
        flashlight: String::new(),
        ..KeyBindings::default()
    };
    bindings.sanitize();
    assert!(
        bindings.forward.is_empty(),
        "repair cannot steal Backward's W"
    );
    assert_eq!(bindings.backward, "W", "valid keys are canonicalized");
    assert_eq!(
        bindings.strafe_left, "A",
        "reserved keypad minus must repair"
    );
    assert!(
        bindings.flashlight.is_empty(),
        "unbound must survive reload"
    );
    assert!(
        bindings.set_key("flashlight", "not a native key").is_err(),
        "unknown native names cannot bind"
    );
    assert!(
        bindings.set_key("flashlight", "KP_MINUS").is_err(),
        "legacy keypad alias remains reserved"
    );
    assert!(
        bindings.set_key("flashlight", "F8").is_err(),
        "diagnostic selector cannot have two owners"
    );
    assert!(
        bindings.set_key("flashlight", "`").is_err(),
        "console owns backtick"
    );
    bindings.set_key("console", "F2")?;
    bindings.set_key("flashlight", "`")?;
    bindings.set_key("interact", "return")?;
    assert_eq!(
        bindings.interact, "ENTER",
        "SDL aliases resolve to emitted key names"
    );
    assert_eq!(
        canonical_key_name("KP_MINUS").as_deref(),
        Some("KEYPAD -"),
        "legacy keypad alias resolves to native spelling"
    );
    let mut reloaded: KeyBindings = serde_json::from_str(&serde_json::to_string(&bindings)?)?;
    reloaded.sanitize();
    assert_eq!(
        reloaded, bindings,
        "repaired controls and unbound actions survive reload"
    );
    Ok(())
}

// One tiny disk roundtrip per style uniquely protects aesthetic/quality separation,
// backward-compatible defaults and master-zero persistence. No native audio/bake.
#[test]
fn styles_preserve_independent_quality_and_master_zero() -> Result<(), Box<dyn std::error::Error>> {
    use crate::game::settings::VisualStyle;
    let directory = SettingsDirectory::new()?;
    let path = directory.0.join("style.json");
    let mut settings = Settings::default();
    let _master_changed = settings.set_master_volume(0.0);
    let _quality_changed = settings.set_quality(QualityLevel::High);
    let _lighting_changed = settings.set_lightmap_quality(LightmapQuality::Full);
    let _filtering_changed = settings.set_texture_filtering("high");
    let quality = settings.graphics_spec();
    for style in [
        VisualStyle::Standard,
        VisualStyle::Crt,
        VisualStyle::Legacy,
        VisualStyle::Vhs,
    ] {
        settings.visual_style = style;
        settings.save_to_path(&path)?;
        let loaded = Settings::load_or_default_from_path(&path);
        assert_eq!(loaded.visual_style, style);
        assert_eq!(loaded.graphics_spec(), quality);
        assert!(
            loaded.master_volume.abs() < f32::EPSILON,
            "zero stays muted after restart"
        );
    }
    let old: Settings = serde_json::from_str("{}")?;
    assert_eq!(old.visual_style, VisualStyle::Standard);
    Ok(())
}

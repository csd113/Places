//! Unit tests for key bindings and settings persistence.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::doc_markdown, clippy::expect_used)]

use super::*;
use crate::test_support::{assert_exact, assert_exact_named};

#[test]
fn test_default_wasd_and_arrow_bindings() {
    let bindings = KeyBindings::default();
    assert_eq!(bindings.forward, "W");
    assert_eq!(bindings.backward, "S");
    assert_eq!(bindings.strafe_left, "A");
    assert_eq!(bindings.strafe_right, "D");
    assert_eq!(bindings.look_up, "UP");
    assert_eq!(bindings.look_down, "DOWN");
    assert_eq!(bindings.look_left, "LEFT");
    assert_eq!(bindings.look_right, "RIGHT");
}

/// The default action map must be exactly WASD + arrows: every key resolves
/// to one action, no key is shared, and the previous PocketCHIP layout is
/// not silently retained as a duplicate binding.
#[test]
fn test_default_action_map_is_wasd_and_arrows_only() {
    let bindings = KeyBindings::default();
    let expected = [
        ("forward", "W"),
        ("backward", "S"),
        ("strafe_left", "A"),
        ("strafe_right", "D"),
        ("look_up", "UP"),
        ("look_down", "DOWN"),
        ("look_left", "LEFT"),
        ("look_right", "RIGHT"),
    ];

    let mut bound_keys: Vec<&str> = Vec::new();
    for (action, key) in expected {
        assert_eq!(bindings.get_key(action), Some(key), "action {action}");
        bound_keys.push(key);
    }

    // No duplicate default keys.
    bound_keys.sort_unstable();
    let unique = {
        let mut keys = bound_keys.clone();
        keys.dedup();
        keys
    };
    assert_eq!(unique.len(), bound_keys.len(), "duplicate default keys");

    // Legacy keys are no longer part of the default layout.
    for legacy in ["Z", "O", ".", "K", "L"] {
        for action in [
            "forward",
            "backward",
            "strafe_left",
            "strafe_right",
            "look_up",
            "look_down",
            "look_left",
            "look_right",
        ] {
            assert_ne!(
                bindings.get_key(action),
                Some(legacy),
                "legacy key {legacy} is still the default for {action}"
            );
        }
    }
}

/// "Restore Default Bindings" rebuilds exactly `KeyBindings::default()`, so
/// resetting after a rebind always lands on WASD + arrows.
#[test]
fn test_reset_to_defaults_restores_wasd_and_arrows() {
    let mut bindings = KeyBindings::default();
    bindings.set_key("forward", "I").expect("rebind forward");
    assert_eq!(bindings.forward, "I");

    // Settings -> "Restore Default Bindings" assigns `KeyBindings::default()`.
    bindings = KeyBindings::default();

    assert_eq!(bindings.forward, "W");
    assert_eq!(bindings.backward, "S");
    assert_eq!(bindings.strafe_left, "A");
    assert_eq!(bindings.strafe_right, "D");
    assert_eq!(bindings.look_up, "UP");
    assert_eq!(bindings.look_down, "DOWN");
    assert_eq!(bindings.look_left, "LEFT");
    assert_eq!(bindings.look_right, "RIGHT");
}

#[test]
fn test_binding_conflict_detection() {
    let mut bindings = KeyBindings::default();
    // Trying to bind forward to "A" (which is strafe_left) must conflict
    assert_eq!(bindings.check_conflict("forward", "A"), Some("strafe_left"));
    assert!(bindings.set_key("forward", "A").is_err());

    // Binding to an unused key like "I" must succeed
    assert_eq!(bindings.check_conflict("forward", "I"), None);
    assert!(bindings.set_key("forward", "I").is_ok());
    assert_eq!(bindings.forward, "I");
}

#[test]
fn test_settings_bounds_sanitization() {
    let mut settings = Settings {
        look_speed_h: 1000.0,
        look_speed_v: -5.0,
        walk_speed: 50.0,
        fov_degrees: 200.0,
        texture_filtering: "bilinear_invalid".to_string(),
        ..Default::default()
    };
    settings.sanitize();

    assert_exact(settings.look_speed_h, 360.0);
    assert_exact(settings.look_speed_v, 20.0);
    assert_exact(settings.walk_speed, 10.0);
    assert_exact(settings.fov_degrees, 110.0);
    assert_eq!(settings.texture_filtering, "linear");
}

#[test]
fn test_quality_profile_defaults_validates_and_round_trips() {
    use crate::quality::QualityProfile;

    // Omitted means the default profile.
    let default = Settings::default();
    assert_eq!(default.quality_profile(), QualityProfile::DEFAULT);
    assert_eq!(default.quality, "full");

    // An unknown profile falls back to the default rather than picking a tier.
    let mut settings = Settings {
        quality: "ultra".to_string(),
        ..Default::default()
    };
    settings.sanitize();
    assert_eq!(settings.quality, "full");

    // Every real profile survives sanitizing, in any case.
    for profile in QualityProfile::ALL {
        let mut settings = Settings {
            quality: profile.name().to_uppercase(),
            ..Default::default()
        };
        settings.sanitize();
        assert_eq!(settings.quality_profile(), profile);
    }

    // A settings file from before profiles existed still loads.
    let legacy = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        }
    }"#;
    let parsed: Settings = serde_json::from_str(legacy).expect("legacy settings parse");
    assert_eq!(parsed.quality_profile(), QualityProfile::DEFAULT);
}

#[test]
fn test_settings_persistence() {
    let temp_dir = std::env::temp_dir();
    let test_path = temp_dir.join("test_liminal_settings.json");

    let settings = Settings {
        look_speed_h: 120.0,
        bindings: KeyBindings {
            forward: "UP".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    settings.save_to_path(&test_path).expect("save settings");
    let loaded = Settings::load_or_default_from_path(&test_path);

    assert_exact(loaded.look_speed_h, 120.0);
    assert_eq!(loaded.bindings.forward, "UP");

    let _ = fs::remove_file(test_path);
}

#[test]
fn test_missing_or_invalid_preferences_fallback() {
    let temp_dir = std::env::temp_dir();
    let missing_path = temp_dir.join("nonexistent_settings.json");
    let default_settings = Settings::default();

    // Nonexistent file returns default
    let loaded_missing = Settings::load_or_default_from_path(&missing_path);
    assert_eq!(loaded_missing, default_settings);

    // Corrupted JSON returns default
    let corrupt_path = temp_dir.join("corrupt_settings.json");
    fs::write(&corrupt_path, "{ broken json ...").expect("write corrupt");
    let loaded_corrupt = Settings::load_or_default_from_path(&corrupt_path);
    assert_eq!(loaded_corrupt, default_settings);

    let _ = fs::remove_file(corrupt_path);
}

/// A hand-edited file can contain a reserved key, an empty name or two actions
/// sharing one key; sanitizing repairs each bad entry in a fixed order.
#[test]
fn test_sanitize_repairs_reserved_empty_and_duplicate_bindings() {
    let raw = r#"{
        "bindings": {
            "forward": "-", "backward": "A", "strafe_left": "A", "strafe_right": "",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        }
    }"#;
    let mut settings: Settings = serde_json::from_str(raw).expect("hand-edited file parses");
    settings.sanitize();

    assert_eq!(settings.bindings.forward, "W", "reserved key falls back");
    assert_eq!(settings.bindings.strafe_left, "A", "first use is kept");
    assert_eq!(
        settings.bindings.backward, "S",
        "duplicate falls back to its default"
    );
    assert_eq!(
        settings.bindings.strafe_right, "D",
        "empty name falls back to its default"
    );
    assert_eq!(settings.bindings.look_up, "UP");
}

/// A malformed settings file is preserved as `settings.json.invalid` and the
/// defaults are returned, so the next run starts from a known state.
#[test]
fn test_malformed_settings_file_is_preserved_and_recovered() {
    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("settings.json");
    fs::write(&path, "{ this is not json").expect("write malformed settings");

    let loaded = Settings::load_or_default_reporting(&path);
    assert_eq!(loaded, Settings::default());
    assert!(!path.exists(), "malformed file is moved aside");
    assert!(
        scratch.join("settings.json.invalid").exists(),
        "the player's file is preserved for inspection"
    );

    // A second load with no file present just returns the defaults.
    let loaded_again = Settings::load_or_default_reporting(&path);
    assert_eq!(loaded_again, Settings::default());

    let _ = fs::remove_file(scratch.join("settings.json.invalid"));
}

/// First run: `ensure_saved_to_path` writes the defaults once, and never
/// overwrites a file that already exists.
#[test]
fn test_ensure_saved_writes_defaults_only_once() {
    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("ensure-saved.json");
    let _ = fs::remove_file(&path);

    Settings::default().ensure_saved_to_path(&path);
    assert!(path.exists(), "first run writes the default file");

    let custom = Settings {
        look_speed_h: 120.0,
        ..Settings::default()
    };
    custom.save_to_path(&path).expect("save custom");
    custom.ensure_saved_to_path(&path);
    let reloaded = Settings::load_or_default_from_path(&path);
    assert_exact_named(
        reloaded.look_speed_h,
        120.0,
        "existing file is not replaced",
    );

    let _ = fs::remove_file(path);
}

/// Action names are shown to the player in the settings prompt and status
/// messages; they must be readable words, not `snake_case` identifiers.
#[test]
fn test_action_labels_are_player_facing() {
    assert_eq!(action_label("forward"), "Forward");
    assert_eq!(action_label("strafe_right"), "Strafe Right");
    assert_eq!(action_label("look_up"), "Look Up");
}

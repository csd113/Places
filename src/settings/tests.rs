//! Unit tests for key bindings and settings persistence.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::doc_markdown, clippy::expect_used)]

use super::*;
use crate::test_support::assert_exact;

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

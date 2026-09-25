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
/// to one action, no key is shared, and no alternative layout is silently
/// retained as a duplicate binding.
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
    assert_eq!(settings.texture_filtering, "high");
}

/// Texture Filtering is persisted as `"low" | "medium" | "high"`, defaults to
/// `"high"`, keeps loading the legacy names and repairs anything unknown.
#[test]
fn test_texture_filtering_defaults_and_legacy_names() {
    let default = Settings::default();
    assert_eq!(default.texture_filtering, "high");
    assert_eq!(TEXTURE_FILTERING_NAMES, ["low", "medium", "high"]);

    // The legacy names keep loading and normalize to the current equivalents.
    for (legacy, expected) in [
        ("linear", "high"),
        ("Linear", "high"),
        ("  LINEAR  ", "high"),
        ("nearest", "low"),
        ("NEAREST", "low"),
        ("", "high"),
        ("trilinear", "high"),
        ("bilinear_invalid", "high"),
        ("anisotropic", "high"),
    ] {
        let mut settings = Settings {
            texture_filtering: legacy.to_string(),
            ..Default::default()
        };
        settings.sanitize();
        assert_eq!(
            settings.texture_filtering, expected,
            "legacy/unknown value {legacy:?}"
        );
    }

    // Every current name survives sanitizing, in any case and with padding.
    for name in TEXTURE_FILTERING_NAMES {
        let mut settings = Settings {
            texture_filtering: format!("  {}  ", name.to_uppercase()),
            ..Default::default()
        };
        settings.sanitize();
        assert_eq!(settings.texture_filtering, name);
    }

    // The same repair happens on the real load path: an old settings.json with
    // the legacy names still parses, and sanitizing rewrites them in memory.
    let legacy_json = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        },
        "texture_filtering": "nearest",
        "quality": "full"
    }"#;
    let mut loaded: Settings =
        serde_json::from_str(legacy_json).expect("a legacy settings file parses");
    loaded.sanitize();
    assert_eq!(loaded.texture_filtering, "low");
    assert_eq!(loaded.quality, "high");
}

#[test]
fn test_texture_filtering_step_cycles_low_medium_high_both_ways() {
    assert_eq!(texture_filtering_step("low", 1), "medium");
    assert_eq!(texture_filtering_step("medium", 1), "high");
    assert_eq!(texture_filtering_step("high", 1), "low");
    assert_eq!(texture_filtering_step("high", -1), "medium");
    assert_eq!(texture_filtering_step("medium", -1), "low");
    assert_eq!(texture_filtering_step("low", -1), "high");
    // Legacy and unknown values step from their canonical equivalent.
    assert_eq!(texture_filtering_step("nearest", 1), "medium");
    assert_eq!(texture_filtering_step("linear", 1), "low");
    assert_eq!(texture_filtering_step("bogus", 1), "low");
}

#[test]
fn test_quality_defaults_validates_and_round_trips() {
    use crate::quality::QualityLevel;

    // Omitted means the default level.
    let default = Settings::default();
    assert_eq!(default.quality_level(), QualityLevel::DEFAULT);
    assert_eq!(default.quality, "high");

    // An unknown level falls back to the default rather than picking a tier.
    let mut settings = Settings {
        quality: "ultra".to_string(),
        ..Default::default()
    };
    settings.sanitize();
    assert_eq!(settings.quality, "high");

    // Every real level survives sanitizing, in any case.
    for level in QualityLevel::ALL {
        let mut settings = Settings {
            quality: level.name().to_uppercase(),
            ..Default::default()
        };
        settings.sanitize();
        assert_eq!(settings.quality_level(), level);
    }

    // The legacy profile name is the same presentation as High today.
    let mut legacy_full = Settings {
        quality: "full".to_string(),
        ..Default::default()
    };
    legacy_full.sanitize();
    assert_eq!(legacy_full.quality, "high");
    assert_eq!(legacy_full.quality_level(), QualityLevel::High);

    // A settings file from before the quality field existed still loads.
    let legacy = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        }
    }"#;
    let parsed: Settings = serde_json::from_str(legacy).expect("legacy settings parse");
    assert_eq!(parsed.quality_level(), QualityLevel::DEFAULT);
}

/// Every level and every Texture Filtering name persists through a save/load
/// round trip unchanged.
#[test]
fn test_quality_and_filtering_persist_round_trip() {
    use crate::quality::QualityLevel;

    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("quality-filtering-round-trip.json");

    for level in QualityLevel::ALL {
        for filtering in TEXTURE_FILTERING_NAMES {
            let settings = Settings {
                quality: level.name().to_string(),
                texture_filtering: filtering.to_string(),
                ..Settings::default()
            };
            settings.save_to_path(&path).expect("save settings");
            let mut loaded = Settings::load_or_default_from_path(&path);
            loaded.sanitize();
            assert_eq!(loaded.quality, level.name(), "quality {level:?}");
            assert_eq!(
                loaded.texture_filtering, filtering,
                "filtering {filtering:?} at {level:?}"
            );
            assert_eq!(loaded.quality_level(), level);
        }
    }

    let _ = fs::remove_file(path);
}

#[test]
fn test_settings_persistence() {
    let temp_dir = std::env::temp_dir();
    let test_path = temp_dir.join("test_places_settings.json");

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

/// Places is a desktop game: the one authoritative fresh-install window size is
/// 1920x1080, and `Settings::default()` (and therefore a written settings file)
/// uses it.
#[test]
fn test_the_default_window_is_1920x1080() {
    assert_eq!(DEFAULT_WINDOW_WIDTH, 1920);
    assert_eq!(DEFAULT_WINDOW_HEIGHT, 1080);
    let settings = Settings::default();
    assert_eq!(settings.window_size(), (1920, 1080));
    assert_eq!(settings.window_mode(), WindowMode::Windowed);
    // The aspect is 16:9, the modern desktop target.
    assert_eq!(
        DEFAULT_WINDOW_WIDTH * 9,
        DEFAULT_WINDOW_HEIGHT * 16,
        "the default must be exactly 16:9"
    );
}

/// A settings file written before bloom, reflections, invert-look and the
/// window fields existed still loads, with every missing field defaulted.
#[test]
fn test_legacy_settings_files_receive_modern_defaults() {
    let legacy = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        },
        "look_speed_h": 120.0,
        "quality": "low"
    }"#;
    let parsed: Settings = serde_json::from_str(legacy).expect("legacy settings parse");
    assert_exact(parsed.look_speed_h, 120.0);
    assert_eq!(parsed.quality_level(), crate::quality::QualityLevel::Low);
    assert!(parsed.bloom_enabled(), "bloom defaults on");
    assert!(parsed.reflections_enabled(), "reflections default on");
    assert!(parsed.lightmaps_enabled(), "lightmaps default on");
    assert!(!parsed.invert_look, "look inversion defaults off");
    assert_eq!(parsed.window_mode(), WindowMode::Windowed);
    assert_eq!(parsed.window_size(), (1920, 1080));
}

/// Every new preference survives a save/load round trip.
#[test]
fn test_new_preferences_persist_round_trip() {
    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("round-trip.json");

    let settings = Settings {
        bloom: false,
        reflections: false,
        invert_look: true,
        window_mode: "fullscreen".to_string(),
        window_width: 2560,
        window_height: 1440,
        ..Settings::default()
    };
    settings.save_to_path(&path).expect("save settings");
    let loaded = Settings::load_or_default_from_path(&path);

    assert!(!loaded.bloom_enabled());
    assert!(!loaded.reflections_enabled());
    assert!(loaded.invert_look);
    assert_eq!(loaded.window_mode(), WindowMode::Fullscreen);
    assert_eq!(loaded.window_size(), (2560, 1440));
    // Unrelated values are untouched by the round trip.
    assert_eq!(loaded.quality_level(), crate::quality::QualityLevel::High);
    assert!(loaded.vsync_enabled());

    let _ = fs::remove_file(path);
}

/// A zero, absurd or malformed display value is repaired rather than accepted.
#[test]
fn test_display_values_are_sanitized() {
    let mut settings = Settings {
        window_width: 0,
        window_height: 100_000,
        window_mode: "cinema".to_string(),
        ..Settings::default()
    };
    settings.sanitize();
    assert_eq!(
        settings.window_size(),
        (MIN_WINDOW_EDGE, MAX_WINDOW_EDGE),
        "each edge is clamped into the usable range"
    );
    assert_eq!(
        settings.window_mode(),
        WindowMode::Windowed,
        "an unknown mode falls back to windowed instead of invalidating the file"
    );

    // Every real mode survives, case-insensitively.
    for mode in WindowMode::ALL {
        let mut settings = Settings {
            window_mode: mode.name().to_uppercase(),
            ..Settings::default()
        };
        settings.sanitize();
        assert_eq!(settings.window_mode(), mode);
    }
}

/// A startup override outranks the saved value for one process, and an explicit
/// change in Settings outranks the override and becomes the saved value.
#[test]
fn test_startup_overrides_outrank_saved_values_until_changed() {
    use crate::quality::QualityLevel;

    let mut settings = Settings {
        quality: "full".to_string(),
        bloom: true,
        reflections: true,
        lightmaps: true,
        vsync: true,
        ..Settings::default()
    };
    settings.overrides = StartupOverrides {
        quality: Some(QualityLevel::Low),
        bloom: Some(false),
        reflections: Some(false),
        lightmaps: Some(false),
        vsync: Some(false),
    };

    // The override is what the process runs with...
    assert_eq!(settings.quality_level(), QualityLevel::Low);
    assert!(!settings.bloom_enabled());
    assert!(!settings.reflections_enabled());
    assert!(!settings.lightmaps_enabled());
    assert!(!settings.vsync_enabled());
    assert!(settings.quality_overridden());
    assert!(settings.bloom_overridden());

    // ...and the saved values are untouched until the player chooses.
    assert_eq!(settings.quality, "full");
    assert!(settings.bloom);
    let json = serde_json::to_string(&settings).expect("serialize");
    assert!(
        !json.contains("overrides") && !json.contains("pending"),
        "session-only state must never be written: {json}"
    );
    assert!(json.contains(r#""quality":"full""#));

    // An explicit choice clears the override for that option only.
    assert!(settings.set_quality(QualityLevel::High));
    assert_eq!(settings.quality, "high");
    assert!(!settings.quality_overridden());
    assert_eq!(settings.quality_level(), QualityLevel::High);
    assert!(
        settings.bloom_overridden(),
        "changing quality leaves the other overrides alone"
    );
    assert!(!settings.bloom_enabled());

    // And the change asks for the right subsystem update.
    let apply = settings.take_pending_apply();
    assert!(apply.graphics_rebuild && !apply.renderer_state);
}

/// Independent graphics preferences never rewrite each other: a valid
/// combination stays valid through a save and an apply.
#[test]
fn test_graphics_settings_are_independent() {
    use crate::quality::QualityLevel;

    let mut settings = Settings::default();
    assert!(settings.set_quality(QualityLevel::Low));
    assert!(settings.set_bloom(false));
    assert!(settings.set_reflections(false));
    assert!(settings.set_lightmaps(false));
    assert!(settings.set_vsync(false));

    assert_eq!(settings.quality_level(), QualityLevel::Low);
    assert!(!settings.bloom_enabled());
    assert!(!settings.reflections_enabled());
    assert!(!settings.lightmaps_enabled());
    assert!(!settings.vsync_enabled());

    // Low + Bloom On is a valid combination and comes back without touching
    // the level.
    assert!(settings.set_bloom(true));
    assert_eq!(settings.quality_level(), QualityLevel::Low);

    let apply = settings.take_pending_apply();
    assert!(apply.graphics_rebuild, "quality/lightmaps changed");
    assert!(apply.renderer_state, "bloom/reflections changed");
    assert!(apply.vsync);
    assert!(!apply.window, "no display setting changed");
    assert!(
        !settings.take_pending_apply().any(),
        "taking the record clears it"
    );

    // The quality selector is a proper selector, not a checkbox: it wraps
    // through all three levels in both directions.
    assert_eq!(
        Settings::quality_step(QualityLevel::Low, 1),
        QualityLevel::Medium
    );
    assert_eq!(
        Settings::quality_step(QualityLevel::Medium, 1),
        QualityLevel::High
    );
    assert_eq!(
        Settings::quality_step(QualityLevel::High, 1),
        QualityLevel::Low
    );
    assert_eq!(
        Settings::quality_step(QualityLevel::High, -1),
        QualityLevel::Medium
    );
    assert_eq!(
        Settings::quality_step(QualityLevel::Medium, -1),
        QualityLevel::Low
    );
    assert_eq!(
        Settings::quality_step(QualityLevel::Low, -1),
        QualityLevel::High
    );
}

/// Quality and Texture Filtering are independent selectors: every combination
/// is representable, and choosing one never rewrites the other.
#[test]
fn test_quality_and_texture_filtering_are_independent() {
    use crate::quality::QualityLevel;

    for level in QualityLevel::ALL {
        for filtering in TEXTURE_FILTERING_NAMES {
            let mut settings = Settings::default();
            let changed = settings.set_quality(level);
            assert_eq!(
                changed,
                level != QualityLevel::High,
                "only a real change reports one"
            );
            settings.texture_filtering = filtering.to_string();
            assert_eq!(settings.quality_level(), level);
            assert_eq!(settings.texture_filtering, filtering);
        }
    }

    // Setting one leaves the other exactly as it was.
    let mut settings = Settings {
        texture_filtering: "low".to_string(),
        ..Settings::default()
    };
    assert!(settings.set_quality(QualityLevel::Medium));
    assert_eq!(settings.texture_filtering, "low");
    let next = texture_filtering_step(&settings.texture_filtering, 1);
    settings.texture_filtering = next.to_string();
    assert_eq!(settings.quality_level(), QualityLevel::Medium);
    assert_eq!(settings.texture_filtering, "medium");
    let _ = settings.take_pending_apply();
}

/// Selecting the value already in force changes nothing and owes nothing.
#[test]
fn test_reselecting_the_same_value_is_inert() {
    use crate::quality::QualityLevel;

    let mut settings = Settings::default();
    assert!(!settings.set_quality(QualityLevel::High));
    assert!(!settings.set_bloom(true));
    assert!(!settings.set_reflections(true));
    assert!(!settings.set_lightmaps(true));
    assert!(!settings.set_vsync(true));
    assert!(!settings.set_window_mode(WindowMode::Windowed));
    assert!(!settings.set_window_size(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT));
    assert!(!settings.take_pending_apply().any());
}

/// The size the window actually has is what the menu shows and the file stores,
/// even below the configured minimum: adopting a smaller window records a real
/// window state, while an explicit selection still enforces the usable range.
#[test]
fn test_adopting_the_real_window_size_never_invents_a_larger_one() {
    let mut settings = Settings::default();
    settings.adopt_window_size(640, 360);
    assert_eq!(settings.window_size(), (640, 360));
    settings.adopt_window_size(200, 150);
    assert_eq!(settings.window_size(), (200, 150));
    assert!(
        !settings.take_pending_apply().any(),
        "adopting the real window asks for no window update"
    );
    assert!(settings.set_window_size(10, 10));
    assert_eq!(settings.window_size(), (MIN_WINDOW_EDGE, MIN_WINDOW_EDGE));
    assert!(settings.take_pending_apply().window);
}

/// `restore_defaults` returns every preference to the 1920x1080 desktop
/// default and asks every subsystem to re-read it.
#[test]
fn test_restore_defaults_resets_display_and_graphics() {
    let mut settings = Settings {
        quality: "low".to_string(),
        bloom: false,
        lightmaps: false,
        window_mode: "fullscreen".to_string(),
        window_width: 1280,
        window_height: 720,
        ..Settings::default()
    };
    settings.restore_defaults();
    assert_eq!(settings.window_size(), (1920, 1080));
    assert_eq!(settings.window_mode(), WindowMode::Windowed);
    assert_eq!(settings.quality_level(), crate::quality::QualityLevel::High);
    assert_eq!(settings.texture_filtering, "high");
    assert!(settings.bloom_enabled());
    assert!(settings.lightmaps_enabled());
    assert_eq!(settings.take_pending_apply(), SettingsApply::ALL);
}

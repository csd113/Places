//! Unit tests for key bindings and settings persistence.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(clippy::doc_markdown, clippy::expect_used)]

use super::*;
use crate::quality::{LightmapQuality, ReflectionQuality};
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
    assert_eq!(bindings.jump, "SPACE");
}

/// The default action map must be exactly WASD + arrows + Space: every key
/// resolves to one action, no key is shared, and no alternative layout is
/// silently retained as a duplicate binding.
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
        ("jump", "SPACE"),
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
        for action in KeyBindings::ACTIONS {
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
    assert_eq!(bindings.jump, "SPACE");
}

#[test]
fn test_binding_conflict_detection() {
    let mut bindings = KeyBindings::default();
    // Trying to bind forward to "A" (which is strafe_left) must conflict
    assert_eq!(bindings.check_conflict("forward", "A"), Some("strafe_left"));
    assert!(bindings.set_key("forward", "A").is_err());

    // The jump binding participates in conflict detection like every other
    // action: Space is taken, and binding forward to it is rejected.
    assert_eq!(bindings.check_conflict("jump", "W"), Some("forward"));
    assert_eq!(bindings.check_conflict("forward", "SPACE"), Some("jump"));
    assert!(bindings.set_key("forward", "SPACE").is_err());
    assert!(bindings.set_key("jump", "W").is_err());

    // Binding to an unused key like "I" must succeed
    assert_eq!(bindings.check_conflict("forward", "I"), None);
    assert!(bindings.set_key("forward", "I").is_ok());
    assert_eq!(bindings.forward, "I");
    // And a jump rebind to an unused key succeeds too.
    assert!(bindings.set_key("jump", "J").is_ok());
    assert_eq!(bindings.jump, "J");
}

#[test]
fn test_settings_bounds_sanitization() {
    let mut settings = Settings {
        look_speed_h: 1000.0,
        look_speed_v: -5.0,
        walk_speed: 50.0,
        fov_degrees: 200.0,
        mouse_sensitivity: 9.0,
        texture_filtering: "bilinear_invalid".to_string(),
        ..Default::default()
    };
    settings.sanitize();

    assert_exact(settings.look_speed_h, 360.0);
    assert_exact(settings.look_speed_v, 20.0);
    assert_exact(settings.walk_speed, 10.0);
    assert_exact(settings.fov_degrees, 110.0);
    assert_exact(settings.mouse_sensitivity, MAX_MOUSE_SENSITIVITY);
    assert_eq!(settings.texture_filtering, "high");
}

/// Mouse sensitivity is a positive scalar with a documented range: a stored
/// zero or negative value and a non-finite one both repair to a usable value.
#[test]
fn mouse_sensitivity_is_sanitized_into_its_documented_range() {
    assert_exact(DEFAULT_MOUSE_SENSITIVITY, 0.12);
    assert_exact(MIN_MOUSE_SENSITIVITY, 0.02);
    assert_exact(MAX_MOUSE_SENSITIVITY, 1.0);
    assert_exact(Settings::default().mouse_sensitivity, 0.12);

    for (stored, expected) in [
        (0.0, MIN_MOUSE_SENSITIVITY),
        (-5.0, MIN_MOUSE_SENSITIVITY),
        (0.5, 0.5),
        (100.0, MAX_MOUSE_SENSITIVITY),
        (f32::NAN, DEFAULT_MOUSE_SENSITIVITY),
        (f32::INFINITY, DEFAULT_MOUSE_SENSITIVITY),
    ] {
        let mut settings = Settings {
            mouse_sensitivity: stored,
            ..Settings::default()
        };
        settings.sanitize();
        assert_exact_named(
            settings.mouse_sensitivity,
            expected,
            format!("stored {stored}"),
        );
    }
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
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT",
            "jump": "W"
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
    assert_eq!(
        settings.bindings.jump, "SPACE",
        "the duplicate W falls back to the jump default"
    );
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
    assert_eq!(action_label("jump"), "Jump");
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

/// A settings file written before bloom, invert-look and the window fields
/// existed still loads, with every missing field defaulted.
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
    // The advanced settings were absent, so each derives the Low preset.
    assert_eq!(
        parsed.reflection_quality(),
        crate::quality::ReflectionQuality::Off,
        "missing reflections derive from the saved quality"
    );
    assert_eq!(
        parsed.lightmap_quality(),
        crate::quality::LightmapQuality::Off,
        "missing lightmaps derive from the saved quality"
    );
    assert_eq!(
        parsed.texture_filtering_preset(),
        "low",
        "missing filtering derives from the saved quality"
    );
    assert!(!parsed.invert_look, "look inversion defaults off");
    assert_eq!(parsed.window_mode(), WindowMode::Windowed);
    assert_eq!(parsed.window_size(), (1920, 1080));
    // The jump binding and mouse sensitivity were absent from the file too.
    assert_eq!(
        parsed.bindings.jump, "SPACE",
        "a missing jump key defaults on load"
    );
    assert_exact(parsed.mouse_sensitivity, DEFAULT_MOUSE_SENSITIVITY);
}

/// Every new preference survives a save/load round trip.
#[test]
fn test_new_preferences_persist_round_trip() {
    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("round-trip.json");

    let settings = Settings {
        bloom: false,
        reflections: "off".to_string(),
        invert_look: true,
        mouse_sensitivity: 0.66,
        bindings: KeyBindings {
            jump: "J".to_string(),
            ..KeyBindings::default()
        },
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
    assert_exact(loaded.mouse_sensitivity, 0.66);
    assert_eq!(loaded.bindings.jump, "J", "the jump rebind round-trips");
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
        reflections: "full".to_string(),
        lightmaps: "full".to_string(),
        vsync: true,
        ..Settings::default()
    };
    settings.overrides = StartupOverrides {
        quality: Some(QualityLevel::Low),
        bloom: Some(false),
        reflections: Some(ReflectionQuality::Off),
        lightmaps: Some(LightmapQuality::Off),
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
    assert!(settings.reflection_quality_overridden());
    assert!(settings.lightmap_quality_overridden());

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
    assert!(
        settings.reflection_quality_overridden() && settings.lightmap_quality_overridden(),
        "the force-off switches stay in force until their own option is changed"
    );
    assert!(!settings.reflections_enabled());
    assert!(!settings.lightmaps_enabled());

    // And the change asks for one graphics transaction.
    let apply = settings.take_pending_apply();
    assert!(apply.graphics && !apply.vsync && !apply.window);
}

/// Independent graphics preferences never rewrite each other: a valid
/// combination stays valid through a save and an apply.
#[test]
fn test_graphics_settings_are_independent() {
    use crate::quality::QualityLevel;

    let mut settings = Settings::default();
    assert!(settings.set_quality(QualityLevel::Medium));
    assert!(settings.set_bloom(false));
    assert!(settings.set_reflection_quality(ReflectionQuality::Off));
    assert!(settings.set_lightmap_quality(LightmapQuality::Off));
    assert!(settings.set_vsync(false));

    assert_eq!(settings.quality_level(), QualityLevel::Medium);
    assert!(!settings.bloom_enabled());
    assert!(!settings.reflections_enabled());
    assert!(!settings.lightmaps_enabled());
    assert!(!settings.vsync_enabled());

    // Medium + Bloom On is a valid combination and comes back without touching
    // the level.
    assert!(settings.set_bloom(true));
    assert_eq!(settings.quality_level(), QualityLevel::Medium);

    let apply = settings.take_pending_apply();
    assert!(
        apply.graphics,
        "quality/bloom/lightmaps/reflections changed"
    );
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
/// is representable, an explicit filtering choice is an override, and only a
/// real quality change cascades the preset.
#[test]
fn test_quality_and_texture_filtering_are_independent() {
    use crate::quality::QualityLevel;

    for level in QualityLevel::ALL {
        let mut settings = Settings::default();
        let changed = settings.set_quality(level);
        assert_eq!(
            changed,
            level != QualityLevel::High,
            "only a real change reports one"
        );
        let expected = match level {
            QualityLevel::Low => "low",
            QualityLevel::Medium => "medium",
            QualityLevel::High => "high",
        };
        assert_eq!(
            settings.texture_filtering, expected,
            "an active quality change cascades the filtering preset"
        );
        assert_eq!(settings.quality_level(), level);
    }

    // An explicit advanced override survives re-selecting the same quality;
    // only a real quality change rewrites it.
    let mut settings = Settings::default();
    assert!(settings.set_texture_filtering("low"));
    assert_eq!(settings.quality_level(), QualityLevel::High);
    assert!(
        !settings.set_quality(QualityLevel::High),
        "same level is inert"
    );
    assert_eq!(settings.texture_filtering, "low");
    assert!(settings.set_quality(QualityLevel::Medium));
    assert_eq!(settings.texture_filtering, "medium", "cascade rewrites it");
    let _ = settings.take_pending_apply();
}

/// Selecting the value already in force changes nothing and owes nothing.
#[test]
fn test_reselecting_the_same_value_is_inert() {
    use crate::quality::QualityLevel;

    let mut settings = Settings::default();
    assert!(!settings.set_quality(QualityLevel::High));
    assert!(!settings.set_bloom(true));
    assert!(!settings.set_reflection_quality(ReflectionQuality::Full));
    assert!(!settings.set_lightmap_quality(LightmapQuality::Full));
    assert!(!settings.set_texture_filtering("high"));
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
        lightmaps: "off".to_string(),
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
    assert_eq!(settings.lightmap_quality(), LightmapQuality::Full);
    assert_eq!(settings.reflection_quality(), ReflectionQuality::Full);
    assert_eq!(settings.take_pending_apply(), SettingsApply::ALL);
}

/// A settings file with the mandatory bindings and whatever extra fields a
/// test wants to exercise, as JSON text.
fn settings_json(extra: &str) -> String {
    format!(
        r#"{{
            "bindings": {{
                "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
                "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
            }},
            {extra}
        }}"#
    )
}

/// The exact preset table: an active quality change selects the three Advanced
/// defaults, and the effective getters agree with the saved names.
#[test]
fn test_the_quality_preset_mapping_table_is_exact() {
    use crate::quality::QualityLevel;

    for (level, filtering, lightmaps, reflections) in [
        (
            QualityLevel::Low,
            "low",
            LightmapQuality::Off,
            ReflectionQuality::Off,
        ),
        (
            QualityLevel::Medium,
            "medium",
            LightmapQuality::Medium,
            ReflectionQuality::Medium,
        ),
        (
            QualityLevel::High,
            "high",
            LightmapQuality::Full,
            ReflectionQuality::Full,
        ),
    ] {
        let mut settings = Settings::default();
        settings.set_quality(level);
        assert_eq!(settings.texture_filtering_preset(), filtering);
        assert_eq!(settings.texture_filtering, filtering);
        assert_eq!(settings.lightmap_quality(), lightmaps);
        assert_eq!(settings.lightmaps, lightmaps.name());
        assert_eq!(settings.reflection_quality(), reflections);
        assert_eq!(settings.reflections, reflections.name());
        assert_eq!(settings.lightmaps_enabled(), !lightmaps.is_off());
        assert_eq!(settings.reflections_enabled(), reflections.draws_probes());
    }
}

/// Changing the overall Quality cascades all three Advanced settings and
/// records one graphics transaction, never four.
#[test]
fn test_quality_cascades_the_advanced_presets_and_requests_one_graphics_apply() {
    use crate::quality::QualityLevel;

    let mut settings = Settings::default();
    // A non-default starting point, so even the High stop is a real change.
    assert!(settings.set_texture_filtering("low"));
    assert!(settings.set_lightmap_quality(LightmapQuality::Off));
    assert!(settings.set_reflection_quality(ReflectionQuality::Off));
    let _ = settings.take_pending_apply();

    for (level, filtering, lightmaps, reflections) in [
        (
            QualityLevel::Low,
            "low",
            LightmapQuality::Off,
            ReflectionQuality::Off,
        ),
        (
            QualityLevel::Medium,
            "medium",
            LightmapQuality::Medium,
            ReflectionQuality::Medium,
        ),
        (
            QualityLevel::High,
            "high",
            LightmapQuality::Full,
            ReflectionQuality::Full,
        ),
    ] {
        assert!(settings.set_quality(level), "{level:?} is a real change");
        assert_eq!(settings.texture_filtering_preset(), filtering);
        assert_eq!(settings.lightmap_quality(), lightmaps);
        assert_eq!(settings.reflection_quality(), reflections);
        let apply = settings.take_pending_apply();
        assert!(apply.graphics, "{level:?} owes a graphics transaction");
        assert!(!apply.vsync && !apply.window, "{level:?} owes nothing else");
    }
}

/// An explicit Advanced choice is an override: it never changes the Quality
/// label or saved value, and every combination is representable.
#[test]
fn test_advanced_overrides_never_change_the_quality_label() {
    use crate::quality::QualityLevel;

    let cases = [
        (
            QualityLevel::Low,
            "lightmaps",
            LightmapQuality::Full,
            ReflectionQuality::Off,
            "low",
        ),
        (
            QualityLevel::Low,
            "reflections",
            LightmapQuality::Off,
            ReflectionQuality::Full,
            "low",
        ),
        (
            QualityLevel::Medium,
            "lightmaps",
            LightmapQuality::Off,
            ReflectionQuality::Medium,
            "medium",
        ),
    ];
    for (level, which, lightmaps, reflections, filtering) in cases {
        let mut settings = Settings::default();
        settings.set_quality(level);
        let _ = settings.take_pending_apply();
        if which == "lightmaps" {
            assert!(settings.set_lightmap_quality(lightmaps));
        } else {
            assert!(settings.set_reflection_quality(reflections));
        }
        assert_eq!(settings.quality_level(), level, "the label is untouched");
        assert_eq!(settings.quality, level.name());
        assert_eq!(settings.texture_filtering_preset(), filtering);
    }

    // High + Filtering Low is the fourth documented override combination.
    let mut settings = Settings::default();
    assert_eq!(settings.quality_level(), QualityLevel::High);
    assert!(settings.set_texture_filtering("low"));
    assert_eq!(settings.quality_level(), QualityLevel::High);
    assert_eq!(settings.quality, "high");
    assert_eq!(settings.texture_filtering_preset(), "low");
    assert!(settings.take_pending_apply().graphics);
}

/// A saved override round-trips exactly: Low + Lightmaps Full is not rewritten
/// by the load path.
#[test]
fn test_advanced_overrides_persist_round_trip() {
    use crate::quality::QualityLevel;

    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("advanced-round-trip.json");

    let mut settings = Settings::default();
    settings.set_quality(QualityLevel::Low);
    settings.set_lightmap_quality(LightmapQuality::Full);
    let _ = settings.take_pending_apply();
    settings.save_to_path(&path).expect("save settings");

    let loaded = Settings::load_or_default_from_path(&path);
    assert_eq!(loaded.quality, "low");
    assert_eq!(loaded.quality_level(), QualityLevel::Low);
    assert_eq!(loaded.lightmap_quality(), LightmapQuality::Full);
    assert_eq!(loaded.reflection_quality(), ReflectionQuality::Off);
    assert_eq!(loaded.texture_filtering_preset(), "low");

    // The file stores the player-facing names, not internal values.
    let json = fs::read_to_string(&path).expect("read back");
    assert!(json.contains(r#""quality": "low""#), "{json}");
    assert!(json.contains(r#""lightmaps": "full""#), "{json}");
    assert!(json.contains(r#""reflections": "off""#), "{json}");

    let _ = fs::remove_file(path);
}

/// A file that predates the Advanced keys derives all three from its saved
/// quality; the legacy name `full` means High.
#[test]
fn test_missing_advanced_keys_derive_from_the_saved_quality() {
    use crate::quality::QualityLevel;

    for (quality, level, filtering, lightmaps, reflections) in [
        (
            "low",
            QualityLevel::Low,
            "low",
            LightmapQuality::Off,
            ReflectionQuality::Off,
        ),
        (
            "medium",
            QualityLevel::Medium,
            "medium",
            LightmapQuality::Medium,
            ReflectionQuality::Medium,
        ),
        (
            "high",
            QualityLevel::High,
            "high",
            LightmapQuality::Full,
            ReflectionQuality::Full,
        ),
        (
            "full",
            QualityLevel::High,
            "high",
            LightmapQuality::Full,
            ReflectionQuality::Full,
        ),
    ] {
        let mut settings: Settings =
            serde_json::from_str(&settings_json(&format!(r#""quality": "{quality}""#)))
                .expect("an older settings file parses");
        settings.sanitize();
        assert_eq!(settings.quality_level(), level, "quality {quality:?}");
        assert_eq!(settings.texture_filtering_preset(), filtering);
        assert_eq!(
            settings.lightmap_quality(),
            lightmaps,
            "quality {quality:?}"
        );
        assert_eq!(
            settings.reflection_quality(),
            reflections,
            "quality {quality:?}"
        );
    }
}

/// The legacy booleans map `true` to Full and `false` to Off, and an explicit
/// legacy value beats the derivation from the saved quality.
#[test]
fn test_legacy_boolean_advanced_values_map_to_full_and_off() {
    let mut settings: Settings = serde_json::from_str(&settings_json(
        r#""quality": "low", "lightmaps": true, "reflections": false"#,
    ))
    .expect("a legacy settings file parses");
    settings.sanitize();
    assert_eq!(
        settings.lightmap_quality(),
        LightmapQuality::Full,
        "an explicit legacy true is Full even under Low"
    );
    assert_eq!(
        settings.reflection_quality(),
        ReflectionQuality::Off,
        "an explicit legacy false is Off"
    );

    // The same file under High maps identically.
    let mut high: Settings = serde_json::from_str(&settings_json(
        r#""quality": "high", "lightmaps": false, "reflections": true"#,
    ))
    .expect("a legacy settings file parses");
    high.sanitize();
    assert_eq!(high.lightmap_quality(), LightmapQuality::Off);
    assert_eq!(high.reflection_quality(), ReflectionQuality::Full);
}

/// A legacy `texture_filtering` value is preserved while the missing Advanced
/// keys derive from the saved quality: `nearest` is Low, `linear` is High.
#[test]
fn test_legacy_filtering_is_preserved_while_missing_advanced_keys_derive() {
    let mut nearest: Settings = serde_json::from_str(&settings_json(
        r#""quality": "medium", "texture_filtering": "nearest""#,
    ))
    .expect("a legacy settings file parses");
    nearest.sanitize();
    assert_eq!(nearest.texture_filtering, "low");
    assert_eq!(nearest.texture_filtering_preset(), "low");
    assert_eq!(nearest.lightmap_quality(), LightmapQuality::Medium);
    assert_eq!(nearest.reflection_quality(), ReflectionQuality::Medium);

    let mut linear: Settings = serde_json::from_str(&settings_json(
        r#""quality": "low", "texture_filtering": "linear""#,
    ))
    .expect("a legacy settings file parses");
    linear.sanitize();
    assert_eq!(linear.texture_filtering, "high");
    assert_eq!(linear.texture_filtering_preset(), "high");
}

/// A valid older file is not renamed to `.invalid`: only a file that cannot be
/// parsed is preserved aside.
#[test]
fn test_a_valid_older_advanced_file_is_not_renamed() {
    use crate::quality::QualityLevel;

    let scratch = std::path::Path::new("target/agent-work/tests/settings");
    fs::create_dir_all(scratch).expect("scratch dir is writable");
    let path = scratch.join("legacy-valid.json");
    let invalid = scratch.join("legacy-valid.json.invalid");
    let _ = fs::remove_file(&invalid);
    fs::write(
        &path,
        settings_json(
            r#""quality": "medium", "texture_filtering": "linear", "lightmaps": true, "reflections": false"#,
        ),
    )
    .expect("write the older file");

    let loaded = Settings::load_or_default_reporting(&path);
    assert!(path.exists(), "a valid older file stays in place");
    assert!(!invalid.exists(), "it must not be renamed to .invalid");
    assert_eq!(loaded.quality_level(), QualityLevel::Medium);
    assert_eq!(loaded.texture_filtering_preset(), "high", "legacy linear");
    assert_eq!(loaded.lightmap_quality(), LightmapQuality::Full);
    assert_eq!(loaded.reflection_quality(), ReflectionQuality::Off);

    let _ = fs::remove_file(path);
}

/// Each Advanced setter clears its own startup override and records one
/// graphics apply only on a real change.
#[test]
fn test_advanced_setters_clear_their_startup_override() {
    let mut settings = Settings::default();
    settings.overrides.lightmaps = Some(LightmapQuality::Off);
    settings.overrides.reflections = Some(ReflectionQuality::Off);
    assert!(!settings.lightmaps_enabled());
    assert!(!settings.reflections_enabled());

    assert!(settings.set_lightmap_quality(LightmapQuality::Full));
    assert!(!settings.lightmap_quality_overridden());
    assert_eq!(settings.lightmap_quality(), LightmapQuality::Full);
    assert!(settings.take_pending_apply().graphics);

    assert!(settings.set_reflection_quality(ReflectionQuality::Medium));
    assert!(!settings.reflection_quality_overridden());
    assert_eq!(settings.reflection_quality(), ReflectionQuality::Medium);
    assert!(settings.take_pending_apply().graphics);

    // Re-selecting the value now in force clears nothing and owes nothing.
    assert!(!settings.set_reflection_quality(ReflectionQuality::Medium));
    assert!(!settings.set_lightmap_quality(LightmapQuality::Full));
    assert!(!settings.take_pending_apply().any());
}

/// The `PLACES_BENCH_GRAPHICS_CYCLE` grammar is exact: known settings and
/// values parse, everything else is ignored.
#[test]
fn test_bench_graphics_cycle_grammar() {
    use crate::bench::{GraphicsChange, parse_graphics_change};

    assert_eq!(
        parse_graphics_change("filtering=low"),
        Some(GraphicsChange::Filtering("low"))
    );
    assert_eq!(
        parse_graphics_change(" Filtering = MEDIUM "),
        Some(GraphicsChange::Filtering("medium"))
    );
    assert_eq!(
        parse_graphics_change("lightmaps=full"),
        Some(GraphicsChange::Lightmaps(LightmapQuality::Full))
    );
    assert_eq!(
        parse_graphics_change("lightmaps=off"),
        Some(GraphicsChange::Lightmaps(LightmapQuality::Off))
    );
    assert_eq!(
        parse_graphics_change("reflections=medium"),
        Some(GraphicsChange::Reflections(ReflectionQuality::Medium))
    );
    assert_eq!(
        parse_graphics_change("bloom=off"),
        Some(GraphicsChange::Bloom(false))
    );
    assert_eq!(
        parse_graphics_change("bloom=1"),
        Some(GraphicsChange::Bloom(true))
    );
    // Unknown settings, values and legacy names are ignored rather than
    // silently mapped onto a player-facing preset.
    for malformed in [
        "filtering=nearest",
        "filtering=ultra",
        "lightmaps=on",
        "reflections=1",
        "bloom=maybe",
        "anisotropy=16",
        "filtering",
        "=low",
    ] {
        assert_eq!(
            parse_graphics_change(malformed),
            None,
            "malformed entry {malformed:?}"
        );
    }
}

/// A settings file written before the jump binding existed loads without being
/// renamed, keeps an explicit rebind, and a missing key gets `SPACE`.
#[test]
fn jump_binding_serde_compatibility() {
    let without_jump = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT"
        }
    }"#;
    let parsed: Settings = serde_json::from_str(without_jump).expect("an older file parses");
    assert_eq!(parsed.bindings.jump, "SPACE");

    let with_jump = r#"{
        "bindings": {
            "forward": "W", "backward": "S", "strafe_left": "A", "strafe_right": "D",
            "look_up": "UP", "look_down": "DOWN", "look_left": "LEFT", "look_right": "RIGHT",
            "jump": "J"
        },
        "mouse_sensitivity": 0.44
    }"#;
    let parsed: Settings = serde_json::from_str(with_jump).expect("a newer file parses");
    assert_eq!(parsed.bindings.jump, "J");
    assert_exact(parsed.mouse_sensitivity, 0.44);

    // A round trip through serialization keeps both new fields.
    let json = serde_json::to_string(&parsed).expect("serialize");
    assert!(json.contains(r#""jump":"J""#), "{json}");
    assert!(json.contains(r#""mouse_sensitivity":0.44"#), "{json}");
}

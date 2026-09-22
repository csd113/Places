use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_SETTINGS_PATH: &str = "settings.json";

/// Player-rebindable gameplay key bindings.
///
/// The defaults are the conventional desktop layout: `W`/`A`/`S`/`D` for
/// movement and the arrow keys for looking. The PocketCHIP layout (`Z`/`S`
/// movement with `K`/`L`/`O`/`.` look) remains reachable by rebinding each
/// action in Settings; only the defaults changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBindings {
    pub forward: String,
    pub backward: String,
    pub strafe_left: String,
    pub strafe_right: String,
    pub look_up: String,
    pub look_down: String,
    pub look_left: String,
    pub look_right: String,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            forward: "W".to_string(),
            backward: "S".to_string(),
            strafe_left: "A".to_string(),
            strafe_right: "D".to_string(),
            look_up: "UP".to_string(),
            look_down: "DOWN".to_string(),
            look_left: "LEFT".to_string(),
            look_right: "RIGHT".to_string(),
        }
    }
}

impl KeyBindings {
    /// Returns the assigned key for a given action name.
    #[must_use]
    pub fn get_key(&self, action: &str) -> Option<&str> {
        match action {
            "forward" => Some(&self.forward),
            "backward" => Some(&self.backward),
            "strafe_left" => Some(&self.strafe_left),
            "strafe_right" => Some(&self.strafe_right),
            "look_up" => Some(&self.look_up),
            "look_down" => Some(&self.look_down),
            "look_left" => Some(&self.look_left),
            "look_right" => Some(&self.look_right),
            _ => None,
        }
    }

    /// Checks if a proposed key is already bound to another action.
    /// Returns `Some(conflicting_action_name)` if a conflict is detected.
    #[must_use]
    pub fn check_conflict(&self, target_action: &str, new_key: &str) -> Option<&'static str> {
        let normalized = new_key.trim().to_uppercase();
        let actions = [
            ("forward", &self.forward),
            ("backward", &self.backward),
            ("strafe_left", &self.strafe_left),
            ("strafe_right", &self.strafe_right),
            ("look_up", &self.look_up),
            ("look_down", &self.look_down),
            ("look_left", &self.look_left),
            ("look_right", &self.look_right),
        ];

        for (action, bound_key) in actions {
            if action != target_action && bound_key.trim().to_uppercase() == normalized {
                return Some(action);
            }
        }
        None
    }

    /// Rebinds an action to a new key if there is no conflict.
    /// # Errors
    ///
    /// Returns a message when `action` is not a known binding name or `new_key`
    /// is already bound to another action.
    pub fn set_key(&mut self, action: &str, new_key: &str) -> Result<(), String> {
        if let Some(conflicting) = self.check_conflict(action, new_key) {
            return Err(format!(
                "Key '{new_key}' is already bound to '{conflicting}'"
            ));
        }
        let key = new_key.trim().to_uppercase();
        match action {
            "forward" => self.forward = key,
            "backward" => self.backward = key,
            "strafe_left" => self.strafe_left = key,
            "strafe_right" => self.strafe_right = key,
            "look_up" => self.look_up = key,
            "look_down" => self.look_down = key,
            "look_left" => self.look_left = key,
            "look_right" => self.look_right = key,
            _ => return Err(format!("Unknown action: {action}")),
        }
        Ok(())
    }
}

/// User game preferences and display settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub bindings: KeyBindings,
    #[serde(default = "default_look_speed_h")]
    pub look_speed_h: f32, // deg/s
    #[serde(default = "default_look_speed_v")]
    pub look_speed_v: f32, // deg/s
    #[serde(default = "default_walk_speed")]
    pub walk_speed: f32, // m/s
    #[serde(default = "default_fov")]
    pub fov_degrees: f32, // degrees
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    #[serde(default = "default_filtering")]
    pub texture_filtering: String,
}

const fn default_look_speed_h() -> f32 {
    90.0
}
const fn default_look_speed_v() -> f32 {
    60.0
}
const fn default_walk_speed() -> f32 {
    3.0
}
const fn default_fov() -> f32 {
    60.0
}
const fn default_vsync() -> bool {
    true
}
fn default_filtering() -> String {
    "linear".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bindings: KeyBindings::default(),
            look_speed_h: default_look_speed_h(),
            look_speed_v: default_look_speed_v(),
            walk_speed: default_walk_speed(),
            fov_degrees: default_fov(),
            vsync: default_vsync(),
            texture_filtering: default_filtering(),
        }
    }
}

impl Settings {
    /// Validates and clamps settings values to safe operational ranges.
    pub fn sanitize(&mut self) {
        self.look_speed_h = self.look_speed_h.clamp(30.0, 360.0);
        self.look_speed_v = self.look_speed_v.clamp(20.0, 240.0);
        self.walk_speed = self.walk_speed.clamp(1.0, 10.0);
        self.fov_degrees = self.fov_degrees.clamp(45.0, 110.0);
        if self.texture_filtering != "linear" && self.texture_filtering != "nearest" {
            self.texture_filtering = "linear".to_string();
        }
    }

    /// Saves settings to a JSON file.
    /// # Errors
    ///
    /// Returns the serialization error or the I/O error from writing the file.
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, json)
    }

    /// Loads settings from a JSON file, falling back safely to defaults on missing/corrupt data.
    pub fn load_or_default_from_path<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if !path.exists() {
            return Self::default();
        }

        fs::read_to_string(path).map_or_else(
            |_| Self::default(),
            |content| {
                serde_json::from_str::<Self>(&content).map_or_else(
                    |_| Self::default(),
                    |mut settings| {
                        settings.sanitize();
                        settings
                    },
                )
            },
        )
    }

    /// Loads settings from default path ("settings.json") or creates default.
    #[must_use]
    pub fn load_or_default() -> Self {
        Self::load_or_default_from_path(DEFAULT_SETTINGS_PATH)
    }

    /// Saves current settings to the default path.
    /// # Errors
    ///
    /// Returns the serialization error or the I/O error from writing
    /// [`DEFAULT_SETTINGS_PATH`].
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to_path(DEFAULT_SETTINGS_PATH)
    }
}

#[cfg(test)]
mod tests {
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
}

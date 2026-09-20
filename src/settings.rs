use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_SETTINGS_PATH: &str = "settings.json";

/// PocketCHIP-aligned gameplay key bindings.
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
            backward: "Z".to_string(),
            strafe_left: "A".to_string(),
            strafe_right: "S".to_string(),
            look_up: "Q".to_string(),
            look_down: ".".to_string(),
            look_left: "K".to_string(),
            look_right: "L".to_string(),
        }
    }
}

impl KeyBindings {
    /// Returns the assigned key for a given action name.
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

fn default_look_speed_h() -> f32 {
    90.0
}
fn default_look_speed_v() -> f32 {
    60.0
}
fn default_walk_speed() -> f32 {
    3.0
}
fn default_fov() -> f32 {
    60.0
}
fn default_vsync() -> bool {
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

        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut settings) => {
                    settings.sanitize();
                    settings
                }
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    /// Loads settings from default path ("settings.json") or creates default.
    pub fn load_or_default() -> Self {
        Self::load_or_default_from_path(DEFAULT_SETTINGS_PATH)
    }

    /// Saves current settings to the default path.
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to_path(DEFAULT_SETTINGS_PATH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pocketchip_bindings() {
        let bindings = KeyBindings::default();
        assert_eq!(bindings.forward, "W");
        assert_eq!(bindings.backward, "Z");
        assert_eq!(bindings.strafe_left, "A");
        assert_eq!(bindings.strafe_right, "S");
        assert_eq!(bindings.look_up, "Q");
        assert_eq!(bindings.look_down, ".");
        assert_eq!(bindings.look_left, "K");
        assert_eq!(bindings.look_right, "L");
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

        assert_eq!(settings.look_speed_h, 360.0);
        assert_eq!(settings.look_speed_v, 20.0);
        assert_eq!(settings.walk_speed, 10.0);
        assert_eq!(settings.fov_degrees, 110.0);
        assert_eq!(settings.texture_filtering, "linear");
    }

    #[test]
    fn test_settings_persistence() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join("test_liminal_settings.json");

        let mut settings = Settings::default();
        settings.look_speed_h = 120.0;
        settings.bindings.forward = "UP".to_string();

        settings.save_to_path(&test_path).expect("save settings");
        let loaded = Settings::load_or_default_from_path(&test_path);

        assert_eq!(loaded.look_speed_h, 120.0);
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

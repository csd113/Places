use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const DEFAULT_SETTINGS_PATH: &str = "settings.json";

/// Player-rebindable gameplay key bindings.
///
/// The defaults are the conventional desktop layout: `W`/`A`/`S`/`D` for
/// movement and the arrow keys for looking. The `PocketCHIP` layout (`Z`/`S`
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
mod tests;

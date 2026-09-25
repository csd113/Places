//! The authoritative player settings: what is persisted, what is in force at
//! runtime, and what a change asks the running systems to do.
//!
//! There is exactly one runtime settings object. It carries:
//!
//! * the **saved values** (`settings.json`), which are what
//!   [`Settings::save`] writes back;
//! * the **startup overrides** (`PLACES_QUALITY=low`, `PLACES_NO_BLOOM=1`,
//!   ...), which are session-only and never persisted;
//! * a **pending-apply** record of which subsystems a change affects, so the
//!   menu never reaches into the renderer, the window or the level directly.
//!
//! Precedence, from weakest to strongest:
//!
//! ```text
//! built-in defaults
//!   └─ saved settings.json
//!        └─ explicit startup override (this process only)
//!             └─ an explicit change made in Settings
//! ```
//!
//! A startup override is visible in the menu (it is what the game is actually
//! running with), and any option the player then changes in Settings clears
//! that option's override and persists the player's choice — an explicit
//! action always beats a launch switch, and the override never silently
//! overwrites the saved file.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::quality::QualityProfile;

pub const DEFAULT_SETTINGS_PATH: &str = "settings.json";

/// Width of the window a fresh installation opens with.
///
/// Places is a desktop game: this is the one authoritative default, referenced
/// by window creation, the settings model and the tests. Nothing else may
/// hard-code a startup window dimension.
pub const DEFAULT_WINDOW_WIDTH: u32 = 1920;
/// Height of the window a fresh installation opens with.
pub const DEFAULT_WINDOW_HEIGHT: u32 = 1080;

/// Smallest window edge a persisted or selected size may specify.
pub const MIN_WINDOW_EDGE: u32 = 320;
/// Largest window edge a persisted or selected size may specify.
pub const MAX_WINDOW_EDGE: u32 = 16_384;

/// Environment override selecting the quality profile for one process.
pub const QUALITY_OVERRIDE_ENV: &str = "PLACES_QUALITY";
/// Environment override disabling the bloom stage for one process.
pub const NO_BLOOM_OVERRIDE_ENV: &str = "PLACES_NO_BLOOM";
/// Environment override disabling reflections for one process.
pub const NO_REFLECTIONS_OVERRIDE_ENV: &str = "PLACES_NO_REFLECTIONS";
/// Environment override disabling lightmap baking for one process.
pub const NO_LIGHTMAPS_OVERRIDE_ENV: &str = "PLACES_NO_LIGHTMAPS";

/// Keys the shell owns and a gameplay action may never bind.
///
/// `ESC` opens the pause menu and cancels a rebind; `-` / keypad `-` toggles
/// the performance overlay. Accepting one of these as a gameplay binding would
/// create a control that can never fire, so the rebind is rejected with a
/// message instead.
pub const RESERVED_KEYS: [&str; 3] = ["ESC", "-", "KP_MINUS"];

/// True when `name` (as produced by [`crate::input::keycode_to_str`]) is
/// reserved by the shell.
#[must_use]
pub fn is_reserved_key(name: &str) -> bool {
    let normalized = name.trim().to_uppercase();
    RESERVED_KEYS.contains(&normalized.as_str())
}

/// Player-facing label of a bindable action (`strafe_right` → `Strafe Right`).
///
/// The settings screen shows this instead of the internal `snake_case` name in
/// its prompts and status messages.
#[must_use]
pub fn action_label(action: &str) -> String {
    action
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// How the game window fills the display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum WindowMode {
    /// A resizable desktop window at the configured resolution.
    #[default]
    Windowed,
    /// Borderless fullscreen at the display's own resolution.
    Fullscreen,
}

impl WindowMode {
    /// Every mode, in settings-screen order.
    pub const ALL: [Self; 2] = [Self::Windowed, Self::Fullscreen];

    /// Stable lowercase name, as written in `settings.json`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windowed => "windowed",
            Self::Fullscreen => "fullscreen",
        }
    }

    /// Player-facing label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Windowed => "Windowed",
            Self::Fullscreen => "Fullscreen",
        }
    }

    /// Parses a mode name, case-insensitively. Unknown names are `None`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let trimmed = name.trim();
        Self::ALL
            .into_iter()
            .find(|mode| mode.name().eq_ignore_ascii_case(trimmed))
    }
}

/// Player-rebindable gameplay key bindings.
///
/// The defaults are the conventional desktop layout: `W`/`A`/`S`/`D` for
/// movement and the arrow keys for looking. Each action can be rebound in Settings.
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

    /// The eight bindable actions, in settings-screen order.
    pub const ACTIONS: [&'static str; 8] = [
        "forward",
        "strafe_left",
        "strafe_right",
        "backward",
        "look_up",
        "look_down",
        "look_left",
        "look_right",
    ];

    /// Rebinds an action to a new key if there is no conflict.
    /// # Errors
    ///
    /// Returns a message when `action` is not a known binding name, `new_key`
    /// is reserved by the shell, or `new_key` is already bound to another
    /// action.
    pub fn set_key(&mut self, action: &str, new_key: &str) -> Result<(), String> {
        if is_reserved_key(new_key) {
            return Err(format!(
                "Key '{}' is reserved for pause/overlay",
                new_key.trim().to_uppercase()
            ));
        }
        if let Some(conflicting) = self.check_conflict(action, new_key) {
            return Err(format!(
                "Key '{}' is already bound to '{}'",
                new_key.trim(),
                action_label(conflicting)
            ));
        }
        self.assign(action, new_key)
    }

    /// Assigns a binding without conflict or reservation checks.
    ///
    /// Only [`Self::set_key`] (which validates first) and
    /// [`Self::sanitize`] (which repairs a hand-edited file) may call this.
    fn assign(&mut self, action: &str, new_key: &str) -> Result<(), String> {
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

    /// Repairs a binding set that could not come from the settings screen.
    ///
    /// A hand-edited or corrupted `settings.json` can contain an empty name, a
    /// reserved key or two actions sharing one key. Each bad entry falls back
    /// to its default independently, in a fixed order, so loading is
    /// deterministic and a valid file is never altered.
    pub fn sanitize(&mut self) {
        let defaults = Self::default();
        let mut used: Vec<String> = Vec::new();
        for action in Self::ACTIONS {
            let current = self.get_key(action).map_or("", str::trim);
            let fallback = defaults.get_key(action).unwrap_or("");
            let key = if current.is_empty()
                || is_reserved_key(current)
                || used.iter().any(|seen| seen == &current.to_uppercase())
            {
                fallback.to_string()
            } else {
                current.to_uppercase()
            };
            let _ = self.assign(action, &key);
            used.push(key.trim().to_uppercase());
        }
    }
}

/// Session-only startup overrides parsed from the environment.
///
/// `None` means "not overridden": the saved value is in force. `Some` is the
/// value the process runs with regardless of what `settings.json` says, until
/// the player changes that setting in the menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StartupOverrides {
    pub quality: Option<QualityProfile>,
    pub bloom: Option<bool>,
    pub reflections: Option<bool>,
    pub lightmaps: Option<bool>,
    pub vsync: Option<bool>,
}

/// What a settings change asks the running systems to do.
///
/// The settings screen only mutates [`Settings`]; `main` consumes this record
/// and performs the minimum work each subsystem needs. A flag is set only by a
/// real value change, so re-selecting the current value is inert.
// A settings change really does have four independent subsystem consequences;
// they are not mutually exclusive states, so an enum per flag would be worse.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingsApply {
    /// The quality profile or the lightmap mode changed: the level's GPU
    /// resources (textures, lightmaps, framebuffers) must be rebuilt while the
    /// current game state stays untouched.
    pub graphics_rebuild: bool,
    /// Renderer state that reads settings directly changed (bloom,
    /// reflections).
    pub renderer_state: bool,
    /// The swap interval must be re-applied to the window.
    pub vsync: bool,
    /// The window mode or resolution must be applied.
    pub window: bool,
}

impl SettingsApply {
    /// Every flag set, for a full restore-to-defaults.
    pub const ALL: Self = Self {
        graphics_rebuild: true,
        renderer_state: true,
        vsync: true,
        window: true,
    };

    /// True when any subsystem has to be updated.
    #[must_use]
    pub const fn any(self) -> bool {
        self.graphics_rebuild || self.renderer_state || self.vsync || self.window
    }
}

/// User game preferences and display settings.
///
/// This is the authoritative runtime state: the menu renders these values, the
/// simulation and renderer read effective values through the `*_enabled`
/// getters, and [`Self::save`] writes exactly this structure back to
/// `settings.json` (minus the session-only [`Self::overrides`]).
// The booleans are independent player preferences (VSync, bloom, reflections,
// lightmaps, look inversion), not a state machine: any combination is valid.
#[allow(clippy::struct_excessive_bools)]
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
    /// Flip the vertical look direction. Off is the historical behaviour.
    #[serde(default = "default_invert_look")]
    pub invert_look: bool,
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    #[serde(default = "default_filtering")]
    pub texture_filtering: String,
    /// Runtime quality profile: `"full"` (the intended presentation) or
    /// `"low"` (the same assets, more aggressively downscaled textures).
    #[serde(default = "default_quality")]
    pub quality: String,
    /// Draw the bloom stage, independent of the quality profile. Default on.
    ///
    /// Bloom is a user preference, not a profile: `Full + Off` and
    /// `Low + On` are both valid. When off, the emissive pass and the blur
    /// passes are skipped entirely. `PLACES_NO_BLOOM=1` overrides it for one
    /// process.
    #[serde(default = "default_bloom")]
    pub bloom: bool,
    /// Draw reflections (static probes and the planar mirror). Default on.
    ///
    /// `PLACES_NO_REFLECTIONS=1` overrides it for one process.
    #[serde(default = "default_reflections")]
    pub reflections: bool,
    /// Bake and draw static lightmaps for level geometry. Default on.
    ///
    /// `false` rebuilds the level through the historical vertex-lit path, which
    /// renders exactly the pre-lightmap colours. `PLACES_NO_LIGHTMAPS=1`
    /// overrides it for one process.
    #[serde(default = "default_lightmaps")]
    pub lightmaps: bool,
    /// Windowed or borderless fullscreen. Persisted as `"windowed"` /
    /// `"fullscreen"`; an unknown value falls back to `"windowed"` rather than
    /// invalidating the file.
    #[serde(default = "default_window_mode")]
    pub window_mode: String,
    /// Windowed width in logical pixels (independent of the Retina drawable).
    #[serde(default = "default_window_width")]
    pub window_width: u32,
    /// Windowed height in logical pixels.
    #[serde(default = "default_window_height")]
    pub window_height: u32,
    /// Session-only startup overrides. Never serialized.
    #[serde(skip)]
    pub overrides: StartupOverrides,
    /// Session-only record of subsystem updates a change still owes. Never
    /// serialized.
    #[serde(skip)]
    pub pending: SettingsApply,
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
const fn default_invert_look() -> bool {
    false
}
const fn default_vsync() -> bool {
    true
}
fn default_filtering() -> String {
    "linear".to_string()
}
fn default_quality() -> String {
    QualityProfile::DEFAULT.name().to_string()
}
const fn default_bloom() -> bool {
    true
}
const fn default_reflections() -> bool {
    true
}
const fn default_lightmaps() -> bool {
    true
}
fn default_window_mode() -> String {
    WindowMode::default().name().to_string()
}
const fn default_window_width() -> u32 {
    DEFAULT_WINDOW_WIDTH
}
const fn default_window_height() -> u32 {
    DEFAULT_WINDOW_HEIGHT
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            bindings: KeyBindings::default(),
            look_speed_h: default_look_speed_h(),
            look_speed_v: default_look_speed_v(),
            walk_speed: default_walk_speed(),
            fov_degrees: default_fov(),
            invert_look: default_invert_look(),
            vsync: default_vsync(),
            texture_filtering: default_filtering(),
            quality: default_quality(),
            bloom: default_bloom(),
            reflections: default_reflections(),
            lightmaps: default_lightmaps(),
            window_mode: default_window_mode(),
            window_width: default_window_width(),
            window_height: default_window_height(),
            overrides: StartupOverrides::default(),
            pending: SettingsApply::default(),
        }
    }
}

/// Shared truthiness rule for the `PLACES_*` switches.
fn truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "off"
    )
}

/// Reads a `PLACES_*` switch that *disables* a feature when truthy.
fn env_disable_override(name: &str) -> Option<bool> {
    std::env::var(name).ok().map(|value| !truthy(&value))
}

/// Previous or next index in a cyclic list, never dividing or wrapping.
///
/// Used by the selectors (quality profile, window mode) so a left/right input
/// is a pure, total step. `count` is never zero in practice; the guard keeps
/// the helper total anyway.
fn cycle_index(index: usize, direction: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    if direction < 0 {
        index
            .checked_sub(1)
            .unwrap_or_else(|| count.saturating_sub(1))
    } else {
        let next = index.saturating_add(1);
        if next < count { next } else { 0 }
    }
}

impl Settings {
    /// Validates and clamps settings values to safe operational ranges.
    pub fn sanitize(&mut self) {
        self.bindings.sanitize();
        self.look_speed_h = self.look_speed_h.clamp(30.0, 360.0);
        self.look_speed_v = self.look_speed_v.clamp(20.0, 240.0);
        self.walk_speed = self.walk_speed.clamp(1.0, 10.0);
        self.fov_degrees = self.fov_degrees.clamp(45.0, 110.0);
        if self.texture_filtering != "linear" && self.texture_filtering != "nearest" {
            self.texture_filtering = "linear".to_string();
        }
        // An unknown profile falls back to the default rather than picking a
        // tier the player did not ask for.
        self.quality = QualityProfile::parse(&self.quality)
            .unwrap_or_default()
            .name()
            .to_string();
        // An unknown window mode falls back to windowed rather than making the
        // file unreadable.
        self.window_mode = WindowMode::parse(&self.window_mode)
            .unwrap_or_default()
            .name()
            .to_string();
        self.window_width = self.window_width.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
        self.window_height = self.window_height.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
    }

    /// Parses the environment's startup overrides into this settings object.
    ///
    /// Called once at startup, before the window and renderer are created. The
    /// values are session-only and are never written back by [`Self::save`].
    /// `vsync` is passed in because the benchmark harness owns
    /// `PLACES_VSYNC`, which is only honored for a benchmark run.
    ///
    /// Precedence: an override beats the saved file, and an explicit change in
    /// Settings beats both (see the module documentation).
    pub fn apply_startup_overrides(&mut self, vsync: Option<bool>) {
        self.overrides = StartupOverrides {
            quality: std::env::var(QUALITY_OVERRIDE_ENV)
                .ok()
                .and_then(|value| QualityProfile::parse(&value)),
            bloom: env_disable_override(NO_BLOOM_OVERRIDE_ENV),
            reflections: env_disable_override(NO_REFLECTIONS_OVERRIDE_ENV),
            lightmaps: env_disable_override(NO_LIGHTMAPS_OVERRIDE_ENV),
            vsync,
        };
    }

    /// The quality profile in force.
    ///
    /// The saved value, with a startup override applied for this process. An
    /// unrecognised override value is ignored, exactly like an unrecognised
    /// settings value.
    #[must_use]
    pub fn quality_profile(&self) -> QualityProfile {
        self.overrides
            .quality
            .unwrap_or_else(|| QualityProfile::parse(&self.quality).unwrap_or_default())
    }

    /// Selects the quality profile and persists it as the saved value.
    ///
    /// Clears any startup override for this option: an explicit change in
    /// Settings outranks a launch switch.
    pub fn set_quality(&mut self, profile: QualityProfile) -> bool {
        let changed = self.quality_profile() != profile;
        self.overrides.quality = None;
        self.quality = profile.name().to_string();
        if changed {
            self.pending.graphics_rebuild = true;
        }
        changed
    }

    /// The quality profile a left/right input selects next.
    #[must_use]
    pub fn quality_step(current: QualityProfile, direction: i32) -> QualityProfile {
        let all = QualityProfile::ALL;
        let index = all
            .iter()
            .position(|profile| *profile == current)
            .unwrap_or(0);
        let next = cycle_index(index, direction, all.len());
        all.get(next).copied().unwrap_or(current)
    }

    /// Whether the quality profile is pinned by a startup override.
    #[must_use]
    pub const fn quality_overridden(&self) -> bool {
        self.overrides.quality.is_some()
    }

    /// Whether the bloom stage may run.
    ///
    /// This is the saved `bloom` preference, with the `PLACES_NO_BLOOM`
    /// startup override applied for this process.
    #[must_use]
    pub fn bloom_enabled(&self) -> bool {
        self.overrides.bloom.unwrap_or(self.bloom)
    }

    /// Turns the bloom stage on or off as a player preference.
    pub fn set_bloom(&mut self, enabled: bool) -> bool {
        let changed = self.bloom_enabled() != enabled;
        self.overrides.bloom = None;
        self.bloom = enabled;
        if changed {
            self.pending.renderer_state = true;
        }
        changed
    }

    /// Toggles the bloom stage, returning its new effective state.
    pub fn toggle_bloom(&mut self) -> bool {
        let next = !self.bloom_enabled();
        self.set_bloom(next);
        next
    }

    /// Whether the bloom stage is pinned by a startup override.
    #[must_use]
    pub const fn bloom_overridden(&self) -> bool {
        self.overrides.bloom.is_some()
    }

    /// Whether reflections may be drawn.
    ///
    /// The saved `reflections` preference, with the `PLACES_NO_REFLECTIONS`
    /// startup override applied for this process.
    #[must_use]
    pub fn reflections_enabled(&self) -> bool {
        self.overrides.reflections.unwrap_or(self.reflections)
    }

    /// Turns reflections on or off as a player preference.
    pub fn set_reflections(&mut self, enabled: bool) -> bool {
        let changed = self.reflections_enabled() != enabled;
        self.overrides.reflections = None;
        self.reflections = enabled;
        if changed {
            self.pending.renderer_state = true;
        }
        changed
    }

    /// Toggles reflections, returning the new effective state.
    pub fn toggle_reflections(&mut self) -> bool {
        let next = !self.reflections_enabled();
        self.set_reflections(next);
        next
    }

    /// Whether reflections are pinned by a startup override.
    #[must_use]
    pub const fn reflections_overridden(&self) -> bool {
        self.overrides.reflections.is_some()
    }

    /// Whether lightmaps should be baked for level geometry.
    ///
    /// The saved `lightmaps` preference, with the `PLACES_NO_LIGHTMAPS`
    /// startup override applied for this process.
    #[must_use]
    pub fn lightmaps_enabled(&self) -> bool {
        self.overrides.lightmaps.unwrap_or(self.lightmaps)
    }

    /// Selects whether lightmaps are baked, rebuilding the level through the
    /// other lighting path at the next apply.
    pub fn set_lightmaps(&mut self, enabled: bool) -> bool {
        let changed = self.lightmaps_enabled() != enabled;
        self.overrides.lightmaps = None;
        self.lightmaps = enabled;
        if changed {
            self.pending.graphics_rebuild = true;
        }
        changed
    }

    /// Toggles lightmaps, returning the new effective state.
    pub fn toggle_lightmaps(&mut self) -> bool {
        let next = !self.lightmaps_enabled();
        self.set_lightmaps(next);
        next
    }

    /// Whether lightmaps are pinned by a startup override.
    #[must_use]
    pub const fn lightmaps_overridden(&self) -> bool {
        self.overrides.lightmaps.is_some()
    }

    /// Whether the swap interval should wait for vertical refresh.
    #[must_use]
    pub fn vsync_enabled(&self) -> bool {
        self.overrides.vsync.unwrap_or(self.vsync)
    }

    /// Selects `VSync`, applied to the live window at the next apply.
    pub fn set_vsync(&mut self, enabled: bool) -> bool {
        let changed = self.vsync_enabled() != enabled;
        self.overrides.vsync = None;
        self.vsync = enabled;
        if changed {
            self.pending.vsync = true;
        }
        changed
    }

    /// Toggles `VSync`, returning the new effective state.
    pub fn toggle_vsync(&mut self) -> bool {
        let next = !self.vsync_enabled();
        self.set_vsync(next);
        next
    }

    /// Whether `VSync` is pinned by a startup override.
    #[must_use]
    pub const fn vsync_overridden(&self) -> bool {
        self.overrides.vsync.is_some()
    }

    /// The window mode in force.
    #[must_use]
    pub fn window_mode(&self) -> WindowMode {
        WindowMode::parse(&self.window_mode).unwrap_or_default()
    }

    /// Selects the window mode, applied to the live window at the next apply.
    pub fn set_window_mode(&mut self, mode: WindowMode) -> bool {
        let changed = self.window_mode() != mode;
        self.window_mode = mode.name().to_string();
        if changed {
            self.pending.window = true;
        }
        changed
    }

    /// Cycles the window mode left or right.
    pub fn step_window_mode(&mut self, direction: i32) -> WindowMode {
        let current = self.window_mode();
        let all = WindowMode::ALL;
        let index = all.iter().position(|mode| *mode == current).unwrap_or(0);
        let next = cycle_index(index, direction, all.len());
        let mode = all.get(next).copied().unwrap_or(current);
        self.set_window_mode(mode);
        mode
    }

    /// The windowed size the player selected, in logical pixels.
    #[must_use]
    pub const fn window_size(&self) -> (u32, u32) {
        (self.window_width, self.window_height)
    }

    /// Selects the windowed size, applied at the next apply.
    ///
    /// Dimensions are clamped to [`MIN_WINDOW_EDGE`]..=[`MAX_WINDOW_EDGE`], so
    /// a hand-edited file can never produce an unusable window.
    pub fn set_window_size(&mut self, width: u32, height: u32) -> bool {
        let width = width.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
        let height = height.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
        let changed = self.window_size() != (width, height);
        self.window_width = width;
        self.window_height = height;
        if changed {
            self.pending.window = true;
        }
        changed
    }

    /// Records the window size actually adopted by the running window.
    ///
    /// Used when a requested windowed size does not fit the active display and
    /// is reduced to fit (or when the player drags the window edge): the menu
    /// then shows the real window, and the value is persisted like any other.
    /// Unlike [`Self::set_window_size`] this does not ask for a window update —
    /// the window already has this size — and it only caps the top of the
    /// range, so a window the player shrank below the configured minimum is
    /// still reported and stored as it is.
    pub fn adopt_window_size(&mut self, width: u32, height: u32) {
        self.window_width = width.clamp(1, MAX_WINDOW_EDGE);
        self.window_height = height.clamp(1, MAX_WINDOW_EDGE);
    }

    /// Turns vertical-look inversion on or off.
    ///
    /// A scalar the simulation reads every frame, so no apply flag is needed.
    pub const fn set_invert_look(&mut self, invert: bool) {
        self.invert_look = invert;
    }

    /// Restores every preference to its documented default.
    ///
    /// Startup overrides are dropped with the rest: the defaults are the
    /// player's explicit new choice. Every subsystem is asked to re-apply.
    pub fn restore_defaults(&mut self) {
        *self = Self::default();
        self.pending = SettingsApply::ALL;
    }

    /// Removes and returns the subsystem updates a change still owes.
    #[must_use]
    pub fn take_pending_apply(&mut self) -> SettingsApply {
        std::mem::take(&mut self.pending)
    }

    /// Saves settings to a JSON file.
    /// # Errors
    ///
    /// Returns the serialization error or the I/O error from writing the file.
    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        if let Some(parent) = path.as_ref().parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, json)
    }

    /// Loads settings from a JSON file, falling back safely to defaults on missing/corrupt data.
    ///
    /// A file that cannot be read or parsed yields the defaults; this entry
    /// point is used by tests and callers that want the raw behaviour. The
    /// persistent load path is [`Self::load_or_default`], which additionally
    /// reports and preserves a malformed file.
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

    /// The path of the persistent settings file inside the runtime state root.
    #[must_use]
    pub fn default_path() -> PathBuf {
        crate::assets::state_path(DEFAULT_SETTINGS_PATH)
    }

    /// Loads the persistent settings, reporting a malformed file once.
    ///
    /// An unreadable or unparseable `settings.json` cannot be used, so the
    /// defaults are returned. A file that exists but does not parse is renamed
    /// to `settings.json.invalid` first: the player's data is preserved for
    /// inspection, the next save writes a clean file, and the game never
    /// silently overwrites a file it could not understand.
    #[must_use]
    pub fn load_or_default() -> Self {
        Self::load_or_default_reporting(Self::default_path())
    }

    /// [`Self::load_or_default`] against an explicit path, for tests.
    #[must_use]
    pub fn load_or_default_reporting<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if !path.exists() {
            return Self::default();
        }
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                crate::logging::warn_once(
                    format!("settings-unreadable:{}", path.display()),
                    format!(
                        "[settings] cannot read {}: {error}; using defaults for this session",
                        path.display()
                    ),
                );
                return Self::default();
            }
        };
        match serde_json::from_str::<Self>(&content) {
            Ok(mut settings) => {
                settings.sanitize();
                settings
            }
            Err(error) => {
                let backup = path.with_extension("json.invalid");
                let preserved = fs::rename(path, &backup).is_ok();
                crate::logging::warn_once(
                    format!("settings-invalid:{}", path.display()),
                    format!(
                        "[settings] {} is not a valid settings file ({error}); using defaults{}",
                        path.display(),
                        if preserved {
                            format!(" and keeping the old file as {}", backup.display())
                        } else {
                            String::new()
                        }
                    ),
                );
                Self::default()
            }
        }
    }

    /// Writes the defaults to the state root when no settings file exists yet.
    ///
    /// This makes a genuinely fresh install initialize its configuration
    /// deliberately, so the first documented file the player can edit is
    /// present without having to change a setting first. An existing file —
    /// valid or not — is never overwritten here.
    pub fn ensure_saved(&self) {
        self.ensure_saved_to_path(Self::default_path());
    }

    /// [`Self::ensure_saved`] against an explicit path, for tests.
    pub fn ensure_saved_to_path<P: AsRef<Path>>(&self, path: P) {
        let path = path.as_ref();
        if path.exists() {
            return;
        }
        if let Err(error) = self.save_to_path(path) {
            crate::logging::warn_once(
                format!("settings-unwritable:{}", path.display()),
                format!(
                    "[settings] cannot create {}: {error}; changes will not persist",
                    path.display()
                ),
            );
        }
    }

    /// Saves current settings to the persistent state path.
    /// # Errors
    ///
    /// Returns the serialization error or the I/O error from writing
    /// [`DEFAULT_SETTINGS_PATH`] below the state root.
    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_to_path(Self::default_path())
    }
}

#[cfg(test)]
mod tests;

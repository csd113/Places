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
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::engine::input::canonical_key_name;
use crate::engine::quality::{GraphicsSpec, LightmapQuality, QualityLevel, ReflectionQuality};

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

/// Environment override selecting the quality level for one process.
pub const QUALITY_OVERRIDE_ENV: &str = "PLACES_QUALITY";
/// Environment override disabling the bloom stage for one process.
pub const NO_BLOOM_OVERRIDE_ENV: &str = "PLACES_NO_BLOOM";
/// Environment override disabling reflections for one process.
pub const NO_REFLECTIONS_OVERRIDE_ENV: &str = "PLACES_NO_REFLECTIONS";
/// Environment override disabling lightmap baking for one process.
pub const NO_LIGHTMAPS_OVERRIDE_ENV: &str = "PLACES_NO_LIGHTMAPS";

/// Keys the shell owns and an action may never bind.
///
/// `ESC` opens the pause menu and cancels a rebind; `-` / keypad `-` toggles
/// the performance overlay; F8 selects diagnostic views in diagnostic builds.
/// Accepting one of these as a binding would create conflicting owners, so the
/// rebind is rejected with a message instead.
pub const RESERVED_KEYS: [&str; 4] = ["ESC", "-", "KEYPAD -", "F8"];

/// True when `name` (as produced by [`crate::game::input::keycode_to_str`]) is
/// reserved by the shell.
#[must_use]
pub fn is_reserved_key(name: &str) -> bool {
    canonical_key_name(name).is_some_and(|normalized| RESERVED_KEYS.contains(&normalized.as_str()))
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
/// movement, the arrow keys for looking, `SPACE` to jump (and swim up), `C` to
/// crouch, `E` to interact and `F` to toggle the flashlight.
/// Each action can be rebound in Settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct KeyBindings {
    pub forward: String,
    pub backward: String,
    pub strafe_left: String,
    pub strafe_right: String,
    pub look_up: String,
    pub look_down: String,
    pub look_left: String,
    pub look_right: String,
    /// Jump, and swim upwards while in deep water.
    ///
    /// Added after the first release: a saved file without this key gets the
    /// default rather than invalidating the whole file.
    #[serde(default = "default_jump_binding")]
    pub jump: String,
    /// Toggle crouching (a press toggles the stance; holding does not repeat).
    ///
    /// Added after the first release, like `jump`: a saved file without this
    /// key gets the `C` default.
    #[serde(default = "default_crouch_binding")]
    pub crouch: String,
    /// Interact with the object in reach (press once; holding does not
    /// repeat).
    ///
    /// Added after the first release, like `jump` and `crouch`: a saved file
    /// without this key gets the `E` default without rewriting any binding the
    /// player customized.
    #[serde(default = "default_interact_binding")]
    pub interact: String,
    /// Toggle the handheld flashlight once per press.
    ///
    /// Older saved files receive `F`; if another action already uses that key,
    /// sanitization preserves it and leaves Flashlight unbound for the player
    /// to choose a free key in Controls.
    #[serde(default = "default_flashlight_binding")]
    pub flashlight: String,
    /// Open or close the opt-in developer console. Defaults to backtick.
    pub console: String,
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
            jump: default_jump_binding(),
            crouch: default_crouch_binding(),
            interact: default_interact_binding(),
            flashlight: default_flashlight_binding(),
            console: "`".to_string(),
        }
    }
}

/// The default jump binding. `SPACE` is not a menu navigation key, so it can
/// stay a gameplay binding in every screen.
fn default_jump_binding() -> String {
    "SPACE".to_string()
}

/// The default crouch binding: `C` is beside the WASD movement keys and is not
/// a menu navigation key or a reserved shell key.
fn default_crouch_binding() -> String {
    "C".to_string()
}

/// The default interact binding: `E` is beside the WASD movement keys and is
/// not a menu navigation key or a reserved shell key. A settings file written
/// before Interact existed has no `interact` key, so serde supplies this
/// default without touching any customized binding; if `E` is already taken by
/// a customized binding, [`KeyBindings::sanitize`] leaves Interact unbound
/// rather than overwriting the player's choice.
fn default_interact_binding() -> String {
    "E".to_string()
}

/// The default flashlight binding: `F` is near the movement keys and is not a
/// menu navigation key or a reserved shell key.
fn default_flashlight_binding() -> String {
    "F".to_string()
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
            "jump" => Some(&self.jump),
            "crouch" => Some(&self.crouch),
            "interact" => Some(&self.interact),
            "flashlight" => Some(&self.flashlight),
            "console" => Some(&self.console),
            _ => None,
        }
    }

    /// Checks if a proposed key is already bound to another action.
    /// Returns `Some(conflicting_action_name)` if a conflict is detected.
    #[must_use]
    pub fn check_conflict(&self, target_action: &str, new_key: &str) -> Option<&'static str> {
        let normalized = canonical_key_name(new_key)?;
        Self::ACTIONS.into_iter().find(|action| {
            *action != target_action
                && self
                    .get_key(action)
                    .and_then(canonical_key_name)
                    .is_some_and(|key| key == normalized)
        })
    }

    /// Every bindable action, in settings-screen order.
    pub const ACTIONS: [&'static str; 13] = [
        "forward",
        "strafe_left",
        "strafe_right",
        "backward",
        "look_up",
        "look_down",
        "look_left",
        "look_right",
        "jump",
        "crouch",
        "interact",
        "flashlight",
        "console",
    ];

    /// Rebinds an action to a new key if there is no conflict.
    /// # Errors
    ///
    /// Returns a message when `action` is not a known binding name, `new_key`
    /// is reserved by the shell, or `new_key` is already bound to another
    /// action.
    pub fn set_key(&mut self, action: &str, new_key: &str) -> Result<(), String> {
        if self.get_key(action).is_none() {
            return Err(format!("Unknown action: {action}"));
        }
        let Some(key) = canonical_key_name(new_key) else {
            return Err(format!("Unknown key: {}", new_key.trim()));
        };
        if is_reserved_key(&key) {
            return Err(format!(
                "Key '{key}' is reserved for pause/overlay/diagnostics"
            ));
        }
        if let Some(conflicting) = self.check_conflict(action, &key) {
            return Err(format!(
                "Key '{}' is already bound to '{}'",
                new_key.trim(),
                action_label(conflicting)
            ));
        }
        self.assign(action, &key)
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
            "jump" => self.jump = key,
            "crouch" => self.crouch = key,
            "interact" => self.interact = key,
            "flashlight" => self.flashlight = key,
            "console" => self.console = key,
            _ => return Err(format!("Unknown action: {action}")),
        }
        Ok(())
    }

    /// Repairs a binding set that could not come from the settings screen.
    ///
    /// A hand-edited or corrupted `settings.json` can contain an unknown name,
    /// a reserved key or two actions sharing one key. Valid keys are reserved
    /// first, then each bad entry falls back to its default in a fixed order.
    /// Empty names remain unbound across reloads. When an action's
    /// default is itself already taken — an old file that customized `jump` to
    /// `E` before Interact existed, for example — the action is left unbound
    /// rather than overwriting the existing binding or leaving a duplicate the
    /// input lookup would silently shadow; the player can rebind it in
    /// Controls.
    pub fn sanitize(&mut self) {
        let defaults = Self::default();
        let mut used: Vec<String> = Vec::new();
        let mut repair = Vec::new();
        // Reserve valid authored keys before filling defaults. An invalid
        // early action must not steal a later action's valid custom binding.
        for action in Self::ACTIONS {
            let current = self.get_key(action).map_or("", str::trim);
            let key = if current.is_empty() {
                // Empty is the persisted representation of an unbound action.
                String::new()
            } else if let Some(key) = canonical_key_name(current)
                && !is_reserved_key(&key)
                && !used.contains(&key)
            {
                used.push(key.clone());
                key
            } else {
                repair.push(action);
                String::new()
            };
            self.assign_sanitized(action, &key);
        }
        for action in repair {
            let fallback = defaults.get_key(action).unwrap_or("");
            if !used.iter().any(|key| key == fallback) {
                self.assign_sanitized(action, fallback);
                used.push(fallback.to_string());
            }
        }
    }

    fn assign_sanitized(&mut self, action: &str, key: &str) {
        if let Err(error) = self.assign(action, key) {
            crate::engine::logging::warn(format!("[settings] could not sanitize binding: {error}"));
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
    pub quality: Option<QualityLevel>,
    pub bloom: Option<bool>,
    pub reflections: Option<ReflectionQuality>,
    pub lightmaps: Option<LightmapQuality>,
    pub vsync: Option<bool>,
}

/// What a settings change asks the running systems to do.
///
/// The settings screen only mutates [`Settings`]; `main` consumes this record
/// and performs the minimum work each subsystem needs. A flag is set only by a
/// real value change, so re-selecting the current value is inert.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "These flags track independent subsystem updates, not mutually exclusive states."
)]
pub struct SettingsApply {
    /// The requested graphics configuration changed (quality, filtering,
    /// lightmaps, reflections or bloom): apply the whole graphics transaction
    /// once, against the values in [`Settings`] at apply time.
    pub graphics: bool,
    /// The swap interval must be re-applied to the window.
    pub vsync: bool,
    /// The window mode or resolution must be applied.
    pub window: bool,
    /// The volume levels or mute preference must reach the audio mixer.
    pub audio: bool,
}

impl SettingsApply {
    /// Every flag set, for a full restore-to-defaults.
    pub const ALL: Self = Self {
        graphics: true,
        vsync: true,
        window: true,
        audio: true,
    };

    /// True when any subsystem has to be updated.
    #[must_use]
    pub const fn any(self) -> bool {
        self.graphics || self.vsync || self.window || self.audio
    }
}

/// User game preferences, display and audio settings.
///
/// This is the authoritative runtime state: the menu renders these values, the
/// simulation and renderer read effective values through the `*_enabled`
/// getters, and [`Self::save`] writes exactly this structure back to
/// `settings.json` (minus the session-only [`Self::overrides`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
// These are independent player preferences, not mutually exclusive states.
#[expect(
    clippy::struct_excessive_bools,
    reason = "These are independent player preferences, not mutually exclusive states."
)]
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
    /// Opt-in developer console, opened by its configured control binding.
    #[serde(default)]
    pub developer_console: bool,
    /// Mouse look sensitivity: degrees of camera rotation per pixel of relative
    /// mouse motion. Pixel motion carries no time, so the simulation applies it
    /// without a delta-time factor; `0.12` is the fresh-install default.
    #[serde(default = "default_mouse_sensitivity")]
    pub mouse_sensitivity: f32,
    /// Overall audio level, from silent (`0.0`) to full volume (`1.0`).
    #[serde(default = "default_audio_volume")]
    pub master_volume: f32,
    /// Interaction and movement sound level before the master volume.
    #[serde(default = "default_audio_volume")]
    pub effects_volume: f32,
    /// Environmental sound level before the master volume.
    #[serde(default = "default_audio_volume")]
    pub ambient_volume: f32,
    /// Silence all audio while preserving the selected volume levels.
    #[serde(default)]
    pub muted: bool,
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    /// Texture Filtering preference: `"low"`, `"medium"` or `"high"`.
    ///
    /// The three levels are independent of the quality level: any combination
    /// is valid, and only the renderer maps them onto its samplers. A missing
    /// key derives its preset from the saved quality, and an unknown value
    /// falls back to the default (High) rather than invalidating the file.
    #[serde(default = "default_advanced_quality")]
    pub texture_filtering: String,
    /// Runtime quality level: `"low"`, `"medium"` or `"high"` (the intended
    /// presentation). All three use the same assets; a lower level downscales
    /// textures and the scene target independently of the lighting path.
    ///
    /// An active [`Self::set_quality`] selects the matching Texture Filtering
    /// default while preserving Lighting and Reflections preferences.
    #[serde(default = "default_quality")]
    pub quality: String,
    /// Draw the bloom stage, independent of the quality level. Default on.
    ///
    /// Bloom is a user preference, not a level: `High + Bloom Off` and
    /// `Low + Bloom On` are both valid. When off, the emissive pass and the blur
    /// passes are skipped entirely. `PLACES_NO_BLOOM=1` overrides it for one
    /// process.
    #[serde(default = "default_bloom")]
    pub bloom: bool,
    /// Reflections quality: `"off"`, `"medium"` or `"full"`.
    ///
    /// Independent of texture quality. Static lighting suspends reflections
    /// while preserving this preference; Full lighting restores it. A missing
    /// key derives the legacy preset from saved texture quality.
    /// `PLACES_NO_REFLECTIONS` overrides it for one process.
    #[serde(
        default = "default_advanced_quality",
        deserialize_with = "deserialize_advanced_quality"
    )]
    pub reflections: String,
    /// Lighting path: `"off"` (Static) or `"full"`; old `"medium"` reads as Full.
    ///
    /// Independent of texture quality. A missing key derives the legacy
    /// preset from saved texture quality. `PLACES_NO_LIGHTMAPS`
    /// overrides it for one process.
    #[serde(
        default = "default_advanced_quality",
        deserialize_with = "deserialize_advanced_quality"
    )]
    pub lightmaps: String,
    /// Legacy saved/scripted Low-lighting adapter. It selects Static without
    /// rewriting the saved lighting path; the visible selector clears it.
    #[serde(default)]
    pub use_low_quality_lighting: bool,
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
/// Slowest selectable mouse sensitivity, in degrees per pixel.
pub const MIN_MOUSE_SENSITIVITY: f32 = 0.02;
/// Fastest selectable mouse sensitivity, in degrees per pixel.
pub const MAX_MOUSE_SENSITIVITY: f32 = 1.0;
/// Fresh-install mouse sensitivity, in degrees per pixel.
pub const DEFAULT_MOUSE_SENSITIVITY: f32 = 0.12;
const fn default_mouse_sensitivity() -> f32 {
    DEFAULT_MOUSE_SENSITIVITY
}

/// Keeps in-memory values as safe as decoded preferences; NaN survives clamp.
const fn sanitized_scalar(value: f32, minimum: f32, maximum: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(minimum, maximum)
    } else {
        fallback
    }
}
const fn default_audio_volume() -> f32 {
    1.0
}

/// Normalizes persisted and selected audio levels before they reach playback.
const fn sanitized_audio_volume(volume: f32) -> f32 {
    if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        default_audio_volume()
    }
}

/// Stores a finite audio level and reports a genuine value change.
const fn set_audio_volume(current: &mut f32, volume: f32) -> bool {
    let normalized = sanitized_audio_volume(volume);
    let changed = current.to_bits() != normalized.to_bits();
    *current = normalized;
    changed
}

const fn default_vsync() -> bool {
    true
}
/// The sentinel a missing advanced-quality key deserializes to.
///
/// [`Settings::sanitize`] resolves it from the saved quality level; it is
/// never written back (a save happens after the runtime settings have been
/// sanitized, or after an explicit setter).
const AUTO_QUALITY: &str = "auto";
fn default_advanced_quality() -> String {
    AUTO_QUALITY.to_string()
}
fn default_quality() -> String {
    QualityLevel::DEFAULT.name().to_string()
}
const fn default_bloom() -> bool {
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
            developer_console: false,
            mouse_sensitivity: default_mouse_sensitivity(),
            master_volume: default_audio_volume(),
            effects_volume: default_audio_volume(),
            ambient_volume: default_audio_volume(),
            muted: false,
            vsync: default_vsync(),
            texture_filtering: "high".to_string(),
            quality: default_quality(),
            bloom: default_bloom(),
            reflections: ReflectionQuality::DEFAULT.name().to_string(),
            lightmaps: LightmapQuality::DEFAULT.name().to_string(),
            use_low_quality_lighting: false,
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
    std::env::var(name)
        .ok()
        .and_then(|value| truthy(&value).then_some(false))
}

/// Reads a `PLACES_NO_*` switch that pins an advanced quality for one process.
///
/// A value naming a level (`off`/`medium`/`full`) selects that level exactly;
/// any other truthy value (`1`, `true`, ...) is the documented "force off"
/// switch, and a falsy value leaves the saved setting in charge.
fn env_quality_override<T: Copy>(
    name: &str,
    off: T,
    parse: impl Fn(&str) -> Option<T>,
) -> Option<T> {
    let value = std::env::var(name).ok()?;
    parse(&value).or_else(|| truthy(&value).then_some(off))
}

/// The three player-facing Texture Filtering levels, in selector order.
///
/// The settings layer stores these as plain strings; the renderer owns the
/// sampler mapping and the GPU type behind it, so nothing here imports a
/// renderer type.
pub const TEXTURE_FILTERING_NAMES: [&str; 3] = ["low", "medium", "high"];

/// The canonical persisted name of any Texture Filtering value.
///
/// The three names round-trip. An empty or unknown value is High, the default.
#[must_use]
pub fn texture_filtering_name(value: &str) -> &'static str {
    let trimmed_value = value.trim();
    if trimmed_value.eq_ignore_ascii_case("low") {
        "low"
    } else if trimmed_value.eq_ignore_ascii_case("medium") {
        "medium"
    } else {
        "high"
    }
}

/// The Texture Filtering name a left/right input selects next.
#[must_use]
pub fn texture_filtering_step(current: &str, direction: i32) -> &'static str {
    let current_name = texture_filtering_name(current);
    let all = TEXTURE_FILTERING_NAMES;
    let index = all
        .iter()
        .position(|name| *name == current_name)
        .unwrap_or_else(|| all.len().saturating_sub(1));
    let next = cycle_index(index, direction, all.len());
    all.get(next).copied().unwrap_or(current_name)
}

/// The Texture Filtering preset an overall quality level selects.
const fn texture_filtering_for(quality: QualityLevel) -> &'static str {
    match quality {
        QualityLevel::Low => "low",
        QualityLevel::Medium => "medium",
        QualityLevel::High => "high",
    }
}

/// Resolves a persisted Texture Filtering value against the saved quality.
///
/// A missing key (the `auto` sentinel) derives its preset from the saved
/// quality level; a present value is preserved through
/// [`texture_filtering_name`].
fn resolve_texture_filtering(value: &str, quality: QualityLevel) -> &'static str {
    if value.trim().eq_ignore_ascii_case(AUTO_QUALITY) {
        texture_filtering_for(quality)
    } else {
        texture_filtering_name(value)
    }
}

/// Resolves a persisted Lightmaps value against the saved quality.
///
/// A missing key derives the legacy preset from saved texture quality.
/// Medium settings migrate to Full; package parsing retains the old quality.
fn resolve_lightmap_quality(value: &str, quality: QualityLevel) -> LightmapQuality {
    let resolved = if value.trim().eq_ignore_ascii_case(AUTO_QUALITY) {
        LightmapQuality::default_for(quality)
    } else {
        LightmapQuality::parse(value).unwrap_or_else(|| LightmapQuality::default_for(quality))
    };
    selectable_lightmaps(resolved)
}

const fn selectable_lightmaps(value: LightmapQuality) -> LightmapQuality {
    match value {
        LightmapQuality::Off => LightmapQuality::Off,
        LightmapQuality::Medium | LightmapQuality::Full => LightmapQuality::Full,
    }
}

/// Resolves a persisted Reflections value against the saved quality.
///
/// A missing key (the `auto` sentinel) derives its preset from the saved
/// quality level; a present value is preserved, and an unknown value falls
/// back to the same preset rather than invalidating the file.
fn resolve_reflection_quality(value: &str, quality: QualityLevel) -> ReflectionQuality {
    if value.trim().eq_ignore_ascii_case(AUTO_QUALITY) {
        ReflectionQuality::default_for(quality)
    } else {
        ReflectionQuality::parse(value).unwrap_or_else(|| ReflectionQuality::default_for(quality))
    }
}

/// Deserializes an advanced graphics-quality value.
///
/// Accepts the level names as written. A `null` value reads as missing and is
/// resolved from the saved quality by [`Settings::sanitize`].
fn deserialize_advanced_quality<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct AdvancedQualityVisitor;

    impl serde::de::Visitor<'_> for AdvancedQualityVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a quality name")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(AUTO_QUALITY.to_string())
        }
    }

    deserializer.deserialize_any(AdvancedQualityVisitor)
}

/// Previous or next index in a cyclic list, never dividing or wrapping.
///
/// Used by the selectors (quality level, texture filtering, window mode) so a
/// left/right input is a pure, total step. `count` is never zero in practice;
/// the guard keeps the helper total anyway.
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
        self.look_speed_h =
            sanitized_scalar(self.look_speed_h, 30.0, 360.0, default_look_speed_h());
        self.look_speed_v =
            sanitized_scalar(self.look_speed_v, 20.0, 240.0, default_look_speed_v());
        self.walk_speed = sanitized_scalar(self.walk_speed, 1.0, 10.0, default_walk_speed());
        self.fov_degrees = sanitized_scalar(self.fov_degrees, 45.0, 110.0, default_fov());
        // A non-finite sensitivity (hand-edited memory or a future decoder)
        // falls back to the default rather than poisoning the camera with NaN.
        self.mouse_sensitivity = if self.mouse_sensitivity.is_finite() {
            self.mouse_sensitivity
                .clamp(MIN_MOUSE_SENSITIVITY, MAX_MOUSE_SENSITIVITY)
        } else {
            DEFAULT_MOUSE_SENSITIVITY
        };
        self.master_volume = sanitized_audio_volume(self.master_volume);
        self.effects_volume = sanitized_audio_volume(self.effects_volume);
        self.ambient_volume = sanitized_audio_volume(self.ambient_volume);
        // An unknown level falls back to the default rather than picking a
        // tier the player did not ask for.
        let quality = QualityLevel::parse(&self.quality).unwrap_or_default();
        self.quality = quality.name().to_string();
        // The advanced settings derive from the saved quality when their key
        // was absent from the file (`auto`); an explicit value is never
        // overwritten here. Only an active `set_quality` cascades.
        self.texture_filtering =
            resolve_texture_filtering(&self.texture_filtering, quality).to_string();
        self.lightmaps = resolve_lightmap_quality(&self.lightmaps, quality)
            .name()
            .to_string();
        self.reflections = resolve_reflection_quality(&self.reflections, quality)
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
                .and_then(|value| QualityLevel::parse(&value)),
            bloom: env_disable_override(NO_BLOOM_OVERRIDE_ENV),
            reflections: env_quality_override(
                NO_REFLECTIONS_OVERRIDE_ENV,
                ReflectionQuality::Off,
                ReflectionQuality::parse,
            ),
            lightmaps: env_quality_override(
                NO_LIGHTMAPS_OVERRIDE_ENV,
                LightmapQuality::Off,
                LightmapQuality::parse,
            ),
            vsync,
        };
    }

    /// The quality level in force.
    ///
    /// The saved value, with a startup override applied for this process. An
    /// unrecognised override value is ignored, exactly like an unrecognised
    /// settings value, which falls back to the default level.
    #[must_use]
    pub fn quality_level(&self) -> QualityLevel {
        self.overrides
            .quality
            .unwrap_or_else(|| QualityLevel::parse(&self.quality).unwrap_or_default())
    }

    /// The lighting path in force, independent of texture quality.
    #[must_use]
    pub fn lighting_quality(&self) -> QualityLevel {
        if self.lightmap_quality().is_off() {
            QualityLevel::Low
        } else {
            QualityLevel::High
        }
    }

    /// Selects the persistent Low lighting override and requests a live
    /// graphics transition. Saved advanced preferences are never rewritten.
    pub const fn set_use_low_quality_lighting(&mut self, enabled: bool) -> bool {
        let changed = self.use_low_quality_lighting != enabled;
        self.use_low_quality_lighting = enabled;
        if changed {
            self.pending.graphics = true;
        }
        changed
    }

    /// Selects the quality level and persists it as the saved value.
    ///
    /// Clears any startup override for this option: an explicit change in
    /// Settings outranks a launch switch.
    ///
    /// A change selects matching Texture Filtering and records one graphics
    /// apply. Lighting, Reflections and their startup overrides stay selected.
    pub fn set_quality(&mut self, level: QualityLevel) -> bool {
        let changed = self.quality_level() != level;
        self.overrides.quality = None;
        self.quality = level.name().to_string();
        if changed {
            self.texture_filtering = texture_filtering_for(level).to_string();
            self.pending.graphics = true;
        }
        changed
    }

    /// The quality level a left/right input selects next.
    #[must_use]
    pub fn quality_step(current: QualityLevel, direction: i32) -> QualityLevel {
        let all = QualityLevel::ALL;
        let index = all.iter().position(|level| *level == current).unwrap_or(0);
        let next = cycle_index(index, direction, all.len());
        all.get(next).copied().unwrap_or(current)
    }

    /// Whether the quality level is pinned by a startup override.
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
            self.pending.graphics = true;
        }
        changed
    }

    /// Toggles the bloom stage, returning its new effective state.
    pub fn toggle_bloom(&mut self) -> bool {
        let next = !self.bloom_enabled();
        let _bloom_changed = self.set_bloom(next);
        next
    }

    /// Whether the bloom stage is pinned by a startup override.
    #[must_use]
    pub const fn bloom_overridden(&self) -> bool {
        self.overrides.bloom.is_some()
    }

    /// The saved quality level, ignoring any startup override.
    fn saved_quality(&self) -> QualityLevel {
        QualityLevel::parse(&self.quality).unwrap_or_default()
    }

    /// The Texture Filtering preset in force: `"low"`, `"medium"` or `"high"`.
    ///
    /// A missing (`auto`) or unknown saved value resolves against the saved
    /// quality level through [`texture_filtering_name`].
    #[must_use]
    pub fn texture_filtering_preset(&self) -> &'static str {
        resolve_texture_filtering(&self.texture_filtering, self.saved_quality())
    }

    /// Selects the Texture Filtering preset, canonicalising the stored value.
    ///
    /// A real change records one graphics apply; the caller persists the
    /// choice. There is no startup override for this option.
    pub fn set_texture_filtering(&mut self, filtering: &str) -> bool {
        let next = texture_filtering_name(filtering);
        let changed = self.texture_filtering_preset() != next;
        self.texture_filtering = next.to_string();
        if changed {
            self.pending.graphics = true;
        }
        changed
    }

    /// The Reflections quality in force.
    ///
    /// The saved value, with the `PLACES_NO_REFLECTIONS` startup override
    /// applied for this process; a missing or unknown value resolves against
    /// the saved texture quality. Static lighting takes precedence,
    /// without changing either saved or startup preferences.
    #[must_use]
    pub fn reflection_quality(&self) -> ReflectionQuality {
        if self.lightmap_quality().is_off() {
            return ReflectionQuality::default_for(QualityLevel::Low);
        }
        if let Some(overridden) = self.overrides.reflections {
            return overridden;
        }
        resolve_reflection_quality(&self.reflections, self.saved_quality())
    }

    /// Whether reflections may be drawn.
    ///
    /// True for every quality except [`ReflectionQuality::Off`].
    #[must_use]
    pub fn reflections_enabled(&self) -> bool {
        self.reflection_quality().draws_probes()
    }

    /// Selects the Reflections quality.
    ///
    /// Clears the matching startup override, so an explicit change in Settings
    /// outranks `PLACES_NO_REFLECTIONS`; a real change records one graphics
    /// apply.
    pub fn set_reflection_quality(&mut self, quality: ReflectionQuality) -> bool {
        let previous = self.reflection_quality();
        self.overrides.reflections = None;
        self.reflections = quality.name().to_string();
        let changed = previous != self.reflection_quality();
        if changed {
            self.pending.graphics = true;
        }
        changed
    }

    /// The Reflections quality a left/right input selects next.
    #[must_use]
    pub fn reflection_quality_step(
        current: ReflectionQuality,
        direction: i32,
    ) -> ReflectionQuality {
        let all = ReflectionQuality::ALL;
        let index = all.iter().position(|level| *level == current).unwrap_or(0);
        let next = cycle_index(index, direction, all.len());
        all.get(next).copied().unwrap_or(current)
    }

    /// Whether Reflections is pinned by a startup override.
    #[must_use]
    pub const fn reflection_quality_overridden(&self) -> bool {
        self.overrides.reflections.is_some()
    }

    /// The Lightmaps quality in force.
    ///
    /// The saved value, with the `PLACES_NO_LIGHTMAPS` startup override
    /// applied for this process; a missing or unknown value resolves against
    /// the saved quality level. Low lighting selects the historical vertex-lit
    /// path while enabled, without changing saved or startup preferences.
    #[must_use]
    pub fn lightmap_quality(&self) -> LightmapQuality {
        if self.use_low_quality_lighting {
            return LightmapQuality::default_for(QualityLevel::Low);
        }
        if let Some(overridden) = self.overrides.lightmaps {
            return selectable_lightmaps(overridden);
        }
        resolve_lightmap_quality(&self.lightmaps, self.saved_quality())
    }

    /// Whether lightmaps should be baked and drawn.
    ///
    /// True for every quality except [`LightmapQuality::Off`].
    #[must_use]
    pub fn lightmaps_enabled(&self) -> bool {
        !self.lightmap_quality().is_off()
    }

    /// Selects the Lightmaps quality, rebuilding the level's lighting at the
    /// next apply.
    ///
    /// Clears the matching startup override, so an explicit change in Settings
    /// outranks `PLACES_NO_LIGHTMAPS`; a real change records one graphics
    /// apply.
    pub fn set_lightmap_quality(&mut self, quality: LightmapQuality) -> bool {
        let previous = self.lightmap_quality();
        self.overrides.lightmaps = None;
        self.use_low_quality_lighting = false;
        self.lightmaps = selectable_lightmaps(quality).name().to_string();
        let changed = previous != self.lightmap_quality();
        if changed {
            self.pending.graphics = true;
        }
        changed
    }

    /// The Lightmaps quality a left/right input selects next.
    #[must_use]
    pub fn lightmap_quality_step(current: LightmapQuality, direction: i32) -> LightmapQuality {
        let all = LightmapQuality::ALL;
        let index = all.iter().position(|level| *level == current).unwrap_or(0);
        let next = cycle_index(index, direction, all.len());
        all.get(next).copied().unwrap_or(current)
    }

    /// Whether Lightmaps is pinned by a startup override.
    #[must_use]
    pub const fn lightmap_quality_overridden(&self) -> bool {
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
        let _vsync_changed = self.set_vsync(next);
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
        let _window_mode_changed = self.set_window_mode(mode);
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
        let clamped_width = width.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
        let clamped_height = height.clamp(MIN_WINDOW_EDGE, MAX_WINDOW_EDGE);
        let changed = self.window_size() != (clamped_width, clamped_height);
        self.window_width = clamped_width;
        self.window_height = clamped_height;
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

    /// Selects the overall audio level and requests a mixer update if changed.
    ///
    /// Finite values clamp to `0.0`..=`1.0`; non-finite values use the default.
    pub const fn set_master_volume(&mut self, volume: f32) -> bool {
        let changed = set_audio_volume(&mut self.master_volume, volume);
        self.pending.audio |= changed;
        changed
    }

    /// Selects the effects level, using the same range as the master volume.
    pub const fn set_effects_volume(&mut self, volume: f32) -> bool {
        let changed = set_audio_volume(&mut self.effects_volume, volume);
        self.pending.audio |= changed;
        changed
    }

    /// Selects the ambience level, using the same range as the master volume.
    pub const fn set_ambient_volume(&mut self, volume: f32) -> bool {
        let changed = set_audio_volume(&mut self.ambient_volume, volume);
        self.pending.audio |= changed;
        changed
    }

    /// Silences or restores playback while retaining every selected volume.
    pub const fn set_muted(&mut self, muted: bool) -> bool {
        let changed = self.muted != muted;
        self.muted = muted;
        self.pending.audio |= changed;
        changed
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

    /// The effective graphics configuration as one comparable value.
    ///
    /// Every field is read through its effective getter, so a startup override
    /// is included (that is what the process is really running with), and the
    /// filtering name is canonicalised through
    /// [`texture_filtering_preset`](Self::texture_filtering_preset). This is
    /// the requested side of the game loop's settled-state comparison; the
    /// renderer answers with the same projection for what it holds.
    #[must_use]
    pub fn graphics_spec(&self) -> GraphicsSpec {
        GraphicsSpec {
            quality: self.quality_level(),
            lighting: self.lighting_quality(),
            filtering: self.texture_filtering_preset(),
            bloom: self.bloom_enabled(),
            lightmaps: self.lightmap_quality(),
            reflections: self.reflection_quality(),
        }
    }

    /// Records that the player asked for the current graphics configuration
    /// again.
    ///
    /// Re-selecting a value is not a value change, but it is still an explicit
    /// request: after a transition failed or was cancelled, the renderer may
    /// hold a different configuration than the menu shows, and a re-selection
    /// is the player asking for it to be applied. The game loop treats a
    /// recorded graphics apply as a fresh request (it clears a previous
    /// failure) and compares the request against what the renderer holds, so
    /// this repairs the desync without touching the saved value.
    pub const fn request_graphics_reapply(&mut self) {
        self.pending.graphics = true;
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
        let settings_path = path.as_ref();
        let (temporary, mut file) = create_settings_temporary(settings_path)?;
        let write_result = file
            .write_all(json.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        let result = write_result.and_then(|()| fs::rename(&temporary, settings_path));
        if result.is_err() {
            let _cleanup = fs::remove_file(&temporary);
        }
        result
    }

    /// Loads settings from a JSON file, falling back safely to defaults on missing/corrupt data.
    ///
    /// A file that cannot be read or parsed yields the defaults; this entry
    /// point is used by tests and callers that want the raw behaviour. The
    /// persistent load path is [`Self::load_or_default`], which additionally
    /// reports and preserves a malformed file.
    pub fn load_or_default_from_path<P: AsRef<Path>>(path: P) -> Self {
        let settings_path = path.as_ref();
        if !settings_path.exists() {
            return Self::default();
        }

        fs::read_to_string(settings_path).map_or_else(
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
    /// # Errors
    /// Returns invalid platform storage or override configuration.
    pub fn default_path() -> Result<PathBuf, String> {
        Ok(crate::engine::paths::user()?.settings())
    }

    /// Loads the persistent settings, reporting a malformed file once.
    ///
    /// An unreadable or unparseable `settings.json` cannot be used, so the
    /// defaults are returned. A file that exists but does not parse is preserved
    /// as `settings.json.invalid` (or the next free numbered sibling) first:
    /// the player's data is retained for
    /// inspection, the next save writes a clean file, and the game never
    /// silently overwrites a file it could not understand.
    /// # Errors
    /// Returns a storage configuration error; malformed settings are preserved.
    pub fn load_or_default() -> Result<Self, String> {
        Ok(Self::load_or_default_reporting(Self::default_path()?))
    }

    /// [`Self::load_or_default`] against an explicit path, for tests.
    #[must_use]
    pub fn load_or_default_reporting<P: AsRef<Path>>(path: P) -> Self {
        let settings_path = path.as_ref();
        if !settings_path.exists() {
            return Self::default();
        }
        let content = match fs::read_to_string(settings_path) {
            Ok(content) => content,
            Err(error) => {
                crate::engine::logging::warn_once(
                    format!("settings-unreadable:{}", settings_path.display()),
                    format!(
                        "[settings] cannot read {}: {error}; using defaults for this session",
                        settings_path.display()
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
                let backup = preserve_invalid_settings(settings_path);
                crate::engine::logging::warn_once(
                    format!("settings-invalid:{}", settings_path.display()),
                    format!(
                        "[settings] {} is not a valid settings file ({error}); using defaults{}",
                        settings_path.display(),
                        backup.map_or_else(String::new, |preserved_path| {
                            format!(" and keeping the old file as {}", preserved_path.display())
                        })
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
    /// # Errors
    /// Returns an invalid storage root or first-save failure with its path.
    pub fn ensure_saved(&self) -> Result<(), String> {
        let path = Self::default_path()?;
        if !path.exists() {
            self.save_to_path(&path).map_err(|error| {
                format!("Cannot initialize settings {}: {error}", path.display())
            })?;
        }
        Ok(())
    }

    /// [`Self::ensure_saved`] against an explicit path, for tests.
    pub fn ensure_saved_to_path<P: AsRef<Path>>(&self, path: P) {
        let bindings_path = path.as_ref();
        if bindings_path.exists() {
            return;
        }
        if let Err(error) = self.save_to_path(bindings_path) {
            crate::engine::logging::warn_once(
                format!("settings-unwritable:{}", bindings_path.display()),
                format!(
                    "[settings] cannot create {}: {error}; changes will not persist",
                    bindings_path.display()
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
        self.save_to_path(Self::default_path().map_err(std::io::Error::other)?)
    }
}

/// Stages settings beside their destination so publication never truncates the
/// previous file. Existing interrupted artifacts are never reused or removed.
fn create_settings_temporary(path: &Path) -> std::io::Result<(PathBuf, fs::File)> {
    for attempt in 0_u8..16 {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".partial-{attempt}"));
        let temporary = path.with_file_name(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "all 16 settings staging paths already exist",
    ))
}

/// Preserve successive malformed files without replacing earlier evidence.
fn preserve_invalid_settings(path: &Path) -> Option<PathBuf> {
    for attempt in 0_u8..16 {
        let extension = if attempt == 0 {
            "json.invalid".to_string()
        } else {
            format!("json.invalid-{attempt}")
        };
        let backup = path.with_extension(extension);
        // A hard link publishes without replacement, unlike rename. Only
        // remove the source once its complete contents have been preserved.
        match fs::hard_link(path, &backup) {
            Ok(()) => {
                let _removed = fs::remove_file(path);
                return Some(backup);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

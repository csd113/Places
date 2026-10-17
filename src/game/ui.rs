use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::engine::font::{get_char_uv, get_white_uv};
use crate::engine::render::Vertex;
use crate::game::AppState;
use crate::game::display::{DisplayStatus, resolution_label, step_resolution};
use crate::game::settings::{KeyBindings, Settings, WindowMode};

mod menu;
mod pointer;
pub use menu::{MenuHit, MenuIntent, MenuLayout, menu_layout};
pub use pointer::MenuPointer;

/// Which Settings screen is open.
///
/// The root is a table of contents; the sections group the actual
/// options. Both entry points (the main menu's Settings and the pause menu's
/// Settings) share this screen, so the page is state, not a separate
/// `AppState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    /// Graphics / Display / Audio / Controls, Restore Defaults and Back.
    #[default]
    Root,
    /// Rendering and visual-quality options.
    Graphics,
    /// Window mode and resolution.
    Display,
    /// Volume levels and mute.
    Audio,
    /// Bindings and control preferences.
    Controls,
}

impl SettingsPage {
    /// Page title drawn at the top of the panel.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Root => "SETTINGS",
            Self::Graphics => "GRAPHICS",
            Self::Display => "DISPLAY",
            Self::Audio => "AUDIO",
            Self::Controls => "CONTROLS",
        }
    }

    /// Number of selectable rows on this page.
    ///
    /// The Graphics page's Advanced group changes the count: collapsed it is
    /// one row (`Advanced`), expanded it includes Texture Filtering and
    /// Reflections. Lighting remains visible when collapsed. [`settings_rows`]
    /// generates exactly this many rows; a unit test pins the two together.
    #[must_use]
    pub const fn item_count(self, advanced_expanded: bool) -> usize {
        match self {
            Self::Root => 6,
            Self::Graphics => {
                if advanced_expanded {
                    8
                } else {
                    6
                }
            }
            Self::Display => 3,
            Self::Audio => 5,
            Self::Controls => KeyBindings::ACTIONS.len().saturating_add(8),
        }
    }
}

/// One selectable row on a Settings page.
///
/// The row list is the single source of truth for both drawing and behaviour:
/// [`activate_settings_item`] dispatches on the row's [`SettingsRowKind`], so a
/// row can never do something other than what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsRow {
    pub label: String,
    pub value: String,
    pub kind: SettingsRowKind,
}

/// What activating or adjusting a row does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRowKind {
    /// A value changed with left/right (Enter adjusts one step too).
    Value(SettingsValue),
    /// The Graphics page's collapsible Advanced group header.
    ///
    /// Enter/Activate expands or collapses the group; left/right is ignored.
    /// The expansion state is session-only UI state, never persisted.
    Advanced,
    /// A player-rebindable gameplay binding.
    Binding(&'static str),
    /// A section entry that opens another page.
    Open(SettingsPage),
    /// Restore every persisted preference to its default.
    RestoreDefaults,
    /// Leave the screen (or return to the root page).
    Back,
}

/// The adjustable options, named so behaviour never depends on a row index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsValue {
    GraphicsQuality,
    Bloom,
    Reflections,
    Lightmaps,
    Vsync,
    Filtering,
    WindowMode,
    Resolution,
    MasterVolume,
    EffectsVolume,
    AmbientVolume,
    Mute,
    LookSpeedH,
    LookSpeedV,
    InvertLook,
    DeveloperConsole,
    MouseSensitivity,
    WalkSpeed,
    Fov,
}

/// What activating a Settings row asks the screen to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAction {
    /// Nothing to do beyond persisting the changed value.
    None,
    /// Leave Settings (the root page's Back row).
    Back,
    /// Open another Settings page.
    Open(SettingsPage),
}

/// Menu selection state across screens.
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub main_menu_idx: usize,
    pub level_select_idx: usize,
    pub pause_menu_idx: usize,
    pub settings_idx: usize,
    /// Which Settings page is open (the root by default).
    pub settings_page: SettingsPage,
    /// Whether the Graphics page's Advanced group is expanded.
    ///
    /// Session-only UI state: collapsed by default, reset whenever a Settings
    /// screen is opened, and never persisted. Texture Filtering and Reflections
    /// are hidden while collapsed.
    pub advanced_expanded: bool,
    pub rebinding_action: Option<&'static str>,
    pub status_message: Option<String>,
    /// True when [`Self::status_message`] describes a failure, so screens can
    /// colour it without parsing their own text.
    pub status_is_error: bool,
    pub level_entries: Vec<String>,
    pub current_map: Option<String>,
    pub level_scroll: usize,
    pub settings_scroll: usize,
    pub pointer: MenuPointer,
    pub loading_map: bool,
}

impl UiState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the one-line status shown by the level list and settings screens.
    pub fn set_status(&mut self, message: impl Into<String>, is_error: bool) {
        self.status_message = Some(message.into());
        self.status_is_error = is_error;
    }

    /// Drops any status message (used on screen transitions and retry).
    pub fn clear_status(&mut self) {
        self.status_message = None;
        self.status_is_error = false;
    }

    pub fn cancel_rebinding(&mut self) {
        self.rebinding_action = None;
        self.clear_status();
    }
}

const fn app_state_tag(state: AppState) -> u8 {
    match state {
        AppState::MainMenu => 0,
        AppState::LevelSelect => 1,
        AppState::Settings => 2,
        AppState::Playing => 3,
        AppState::Paused => 4,
        AppState::PauseSettings => 5,
    }
}

/// Hashes every input that can change the generated menu/settings geometry, so
/// an unchanged screen can be reused instead of rebuilt each frame.
fn ui_signature(
    app_state: AppState,
    ui_state: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
    version: &str,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    app_state_tag(app_state).hash(&mut hasher);
    ui_state.main_menu_idx.hash(&mut hasher);
    ui_state.level_select_idx.hash(&mut hasher);
    ui_state.pause_menu_idx.hash(&mut hasher);
    ui_state.settings_idx.hash(&mut hasher);
    ui_state.settings_page.hash(&mut hasher);
    ui_state.advanced_expanded.hash(&mut hasher);
    ui_state.rebinding_action.hash(&mut hasher);
    ui_state.status_message.as_deref().hash(&mut hasher);
    ui_state.status_is_error.hash(&mut hasher);
    ui_state.level_entries.hash(&mut hasher);
    ui_state.current_map.hash(&mut hasher);
    ui_state.level_scroll.hash(&mut hasher);
    ui_state.settings_scroll.hash(&mut hasher);
    ui_state.pointer.hover.hash(&mut hasher);
    ui_state.loading_map.hash(&mut hasher);
    display.hash(&mut hasher);
    version.hash(&mut hasher);

    let b = &settings.bindings;
    b.forward.hash(&mut hasher);
    b.backward.hash(&mut hasher);
    b.strafe_left.hash(&mut hasher);
    b.strafe_right.hash(&mut hasher);
    b.look_up.hash(&mut hasher);
    b.look_down.hash(&mut hasher);
    b.look_left.hash(&mut hasher);
    b.look_right.hash(&mut hasher);
    b.jump.hash(&mut hasher);
    b.crouch.hash(&mut hasher);
    b.interact.hash(&mut hasher);
    b.flashlight.hash(&mut hasher);
    settings.look_speed_h.to_bits().hash(&mut hasher);
    settings.look_speed_v.to_bits().hash(&mut hasher);
    settings.mouse_sensitivity.to_bits().hash(&mut hasher);
    settings.master_volume.to_bits().hash(&mut hasher);
    settings.effects_volume.to_bits().hash(&mut hasher);
    settings.ambient_volume.to_bits().hash(&mut hasher);
    settings.muted.hash(&mut hasher);
    settings.walk_speed.to_bits().hash(&mut hasher);
    settings.fov_degrees.to_bits().hash(&mut hasher);
    settings.invert_look.hash(&mut hasher);
    settings.developer_console.hash(&mut hasher);
    // The *effective* values: a startup override changes what the screen says.
    settings.vsync_enabled().hash(&mut hasher);
    settings.bloom_enabled().hash(&mut hasher);
    settings.reflection_quality().hash(&mut hasher);
    settings.lightmap_quality().hash(&mut hasher);
    settings.quality_level().hash(&mut hasher);
    settings.use_low_quality_lighting.hash(&mut hasher);
    settings.window_mode().hash(&mut hasher);
    settings.texture_filtering_preset().hash(&mut hasher);
    settings.window_size().hash(&mut hasher);
    hasher.finish()
}

/// Caches 2D menu/settings UI geometry.
///
/// A simple signature comparison avoids rebuilding (and re-uploading) identical
/// vertices every frame while a menu is open, without a retained-mode GUI.
#[derive(Default)]
pub struct UiGeometryCache {
    vertices: Vec<Vertex>,
    signature: u64,
    initialized: bool,
}

impl UiGeometryCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the UI vertices for the current state, rebuilding only when a
    /// relevant input changed.
    pub fn get(
        &mut self,
        app_state: AppState,
        ui_state: &UiState,
        settings: &Settings,
        display: &DisplayStatus,
        version: &str,
    ) -> &[Vertex] {
        let signature = ui_signature(app_state, ui_state, settings, display, version);
        if !self.initialized || signature != self.signature {
            self.vertices = build_ui_geometry(app_state, ui_state, settings, display, version);
            self.signature = signature;
            self.initialized = true;
        }
        &self.vertices
    }
}

fn add_ui_quad(
    vertices: &mut Vec<Vertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 4],
    uv: [f32; 4],
) {
    let u0 = uv[0];
    let v0 = uv[1];
    let u1 = uv[2];
    let v1 = uv[3];

    let p0 = [x0, y0, 0.0];
    let p1 = [x1, y0, 0.0];
    let p2 = [x1, y1, 0.0];
    let p3 = [x0, y1, 0.0];

    vertices.push(Vertex {
        pos: p0,
        color,
        uv: [u0, v0],
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p1,
        color,
        uv: [u1, v0],
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p2,
        color,
        uv: [u1, v1],
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p0,
        color,
        uv: [u0, v0],
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p2,
        color,
        uv: [u1, v1],
        ..Vertex::UNLIT
    });
    vertices.push(Vertex {
        pos: p3,
        color,
        uv: [u0, v1],
        ..Vertex::UNLIT
    });
}

pub fn add_rect_rgba(
    vertices: &mut Vec<Vertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 4],
) {
    add_ui_quad(vertices, x0, y0, x1, y1, color, get_white_uv());
}

pub fn add_rect(vertices: &mut Vec<Vertex>, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 3]) {
    add_rect_rgba(
        vertices,
        x0,
        y0,
        x1,
        y1,
        [color[0], color[1], color[2], 1.0],
    );
}

pub fn draw_text(
    vertices: &mut Vec<Vertex>,
    text: &str,
    mut x: f32,
    y: f32,
    scale: f32,
    color: [f32; 3],
) {
    let char_w = 8.0 * scale;
    let char_h = 8.0 * scale;

    for ch in text.chars() {
        if let Some(uv) = get_char_uv(ch)
            && ch != ' '
        {
            add_ui_quad(
                vertices,
                x,
                y,
                x + char_w,
                y + char_h,
                [color[0], color[1], color[2], 1.0],
                uv,
            );
        }
        x += char_w;
    }
}

/// Pixel width of `text` at `scale` in the 480x272 reference space.
///
/// Every glyph — including an unsupported character — advances exactly
/// `8 * scale` pixels, so this is exact rather than an estimate.
#[must_use]
pub fn text_width(text: &str, scale: f32) -> f32 {
    let characters = u16::try_from(text.chars().count()).unwrap_or(u16::MAX);
    f32::from(characters).mul_add(8.0 * scale, 0.0)
}

/// Truncates `text` with a trailing `...` so it fits `max_width` pixels.
///
/// Level names and loader diagnostics are arbitrary strings; a name that would
/// run past the panel is shortened rather than allowed to bleed over the
/// screen edge (there is no scissor rectangle on the UI pass).
#[must_use]
pub fn fit_text(text: &str, max_width: f32, scale: f32) -> String {
    /// Upper bound on the characters a single fitted line may occupy.
    const MAX_FITTED_CHARS: usize = 512;
    let char_w = 8.0 * scale;
    if char_w <= 0.0 {
        return text.to_string();
    }
    let mut max_chars = 0usize;
    let mut used = 0.0_f32;
    while used + char_w <= max_width && max_chars < MAX_FITTED_CHARS {
        used += char_w;
        max_chars = max_chars.saturating_add(1);
    }
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// Appends the committed hollow hand with the current binding inside the palm.
/// The caller owns target eligibility and gameplay visibility.
pub fn append_interaction_hint(vertices: &mut Vec<Vertex>, key: &str) {
    const PALM_CENTER: [f32; 2] = [240.0, 216.0];
    const INK: [f32; 3] = [0.96, 0.94, 0.88];
    const EDGE: [f32; 3] = [0.025, 0.03, 0.03];

    // A fixed square preserves the authored hand, even for long rebound names.
    // Sixteen reference pixels give approximately 30px of ink at 960x544.
    add_ui_quad(
        vertices,
        232.0,
        205.5,
        248.0,
        221.5,
        [1.0; 4],
        crate::engine::font::interaction_hand_uv(),
    );
    let label = fit_text(key, 128.0, 1.0);
    let scale = (7.0 / text_width(&label, 1.0).max(1.0)).min(0.5);
    let left = text_width(&label, scale).mul_add(-0.5, PALM_CENTER[0]);
    let top = scale.mul_add(-3.5, PALM_CENTER[1]);
    for [dx, dy] in [[-0.2, 0.0], [0.2, 0.0], [0.0, -0.2], [0.0, 0.2]] {
        draw_text(vertices, &label, left + dx, top + dy, scale, EDGE);
    }
    draw_text(vertices, &label, left, top, scale, INK);
}

/// Menu vertices and pointer regions share the same reference-space layout.
#[must_use]
pub fn build_ui_geometry(
    app_state: AppState,
    ui_state: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
    version: &str,
) -> Vec<Vertex> {
    menu::geometry(app_state, ui_state, settings, display, version)
}

/// Startup preparation uses the same heading and status treatment as menus.
#[must_use]
pub fn loading_geometry(ui_state: &UiState, version: &str) -> Vec<Vertex> {
    menu::loading(ui_state, version)
}

/// Lossless row offsets for the small menu lists.
fn row_y(index: usize, line_h: f32, start_y: f32) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(u16::MAX)).mul_add(line_h, start_y)
}

/// The section entries the Settings root offers, in menu order.
pub const SETTINGS_SECTIONS: [SettingsPage; 4] = [
    SettingsPage::Graphics,
    SettingsPage::Display,
    SettingsPage::Audio,
    SettingsPage::Controls,
];

impl SettingsPage {
    /// The player-facing label of a section entry.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Root => "Settings",
            Self::Graphics => "Graphics",
            Self::Display => "Display",
            Self::Audio => "Audio",
            Self::Controls => "Controls",
        }
    }
}

/// Suffix appended to a value pinned by a startup override.
const OVERRIDE_MARKER: &str = " *";

const fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

/// Player-facing label of a Texture Filtering value.
///
/// The three names are the whole vocabulary: no filtering, sampler or
/// anisotropy term can ever reach the screen. An unknown value shows
/// its canonical equivalent (High by default).
fn filtering_label(mode: &str) -> &'static str {
    match crate::game::settings::texture_filtering_name(mode) {
        "low" => "Low",
        "medium" => "Medium",
        _ => "High",
    }
}

/// The selector values a row wraps around, rendered as `< value >`.
fn selector(value: &str) -> String {
    format!("< {value} >")
}

/// A selector value with the startup-override marker when one is in force.
fn selector_with_override(value: &str, overridden: bool) -> String {
    if overridden {
        format!("{}{OVERRIDE_MARKER}", selector(value))
    } else {
        selector(value)
    }
}

/// True when any Graphics option is pinned by a startup override, so the
/// Graphics legend can explain the marker.
#[must_use]
pub fn any_graphics_override(settings: &Settings) -> bool {
    settings.quality_overridden()
        || settings.bloom_overridden()
        || (!settings.use_low_quality_lighting && settings.lightmap_quality_overridden())
        || (settings.lightmaps_enabled() && settings.reflection_quality_overridden())
        || settings.vsync_overridden()
}

/// Appends one row to a page's row list.
fn push_row(rows: &mut Vec<SettingsRow>, label: &str, value: String, kind: SettingsRowKind) {
    rows.push(SettingsRow {
        label: label.to_string(),
        value,
        kind,
    });
}

/// Every row of one Settings page, in menu order.
///
/// This is the single source of truth for the screen: the geometry draws these
/// rows and [`activate_settings_item`] dispatches on the same list, so the
/// current value can never be shown without also being the value that changes.
/// `advanced_expanded` only affects the Graphics page, whose Advanced group
/// hides Texture Filtering and Reflections while collapsed.
#[must_use]
pub fn settings_rows(
    page: SettingsPage,
    settings: &Settings,
    display: &DisplayStatus,
    advanced_expanded: bool,
) -> Vec<SettingsRow> {
    match page {
        SettingsPage::Root => root_rows(),
        SettingsPage::Graphics => graphics_rows(settings, advanced_expanded),
        SettingsPage::Display => display_rows(settings, display),
        SettingsPage::Audio => audio_rows(settings),
        SettingsPage::Controls => controls_rows(settings),
    }
}

/// The Settings root: one entry per section, Restore Defaults and Back.
fn root_rows() -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Root.item_count(false));
    for section in SETTINGS_SECTIONS {
        push_row(
            &mut rows,
            section.label(),
            ">".to_string(),
            SettingsRowKind::Open(section),
        );
    }
    push_row(
        &mut rows,
        "Restore Defaults",
        String::new(),
        SettingsRowKind::RestoreDefaults,
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Graphics: the quality level, the always-visible preferences, then the
/// collapsible Advanced group (Texture Filtering and Reflections).
///
/// The advanced rows are the only place their values are offered; the
/// strings are the player-facing labels, never an internal parameter.
fn graphics_rows(settings: &Settings, advanced_expanded: bool) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Graphics.item_count(advanced_expanded));
    push_row(
        &mut rows,
        "Graphics Quality",
        selector_with_override(
            settings.quality_level().label(),
            settings.quality_overridden(),
        ),
        SettingsRowKind::Value(SettingsValue::GraphicsQuality),
    );
    push_row(
        &mut rows,
        "Bloom",
        selector_with_override(
            on_off(settings.bloom_enabled()),
            settings.bloom_overridden(),
        ),
        SettingsRowKind::Value(SettingsValue::Bloom),
    );
    push_row(
        &mut rows,
        "VSync",
        selector_with_override(
            on_off(settings.vsync_enabled()),
            settings.vsync_overridden(),
        ),
        SettingsRowKind::Value(SettingsValue::Vsync),
    );
    push_row(
        &mut rows,
        "Advanced",
        if advanced_expanded { "v" } else { ">" }.to_string(),
        SettingsRowKind::Advanced,
    );
    if advanced_expanded {
        push_row(
            &mut rows,
            "Texture Filtering",
            selector(filtering_label(settings.texture_filtering_preset())),
            SettingsRowKind::Value(SettingsValue::Filtering),
        );
        push_row(
            &mut rows,
            "Reflections",
            if settings.lightmaps_enabled() {
                selector_with_override(
                    settings.reflection_quality().label(),
                    settings.reflection_quality_overridden(),
                )
            } else {
                "Off (Static lighting)".to_string()
            },
            SettingsRowKind::Value(SettingsValue::Reflections),
        );
    }
    push_row(
        &mut rows,
        "Lighting",
        selector_with_override(
            settings.lightmap_quality().label(),
            settings.lightmap_quality_overridden() && !settings.use_low_quality_lighting,
        ),
        SettingsRowKind::Value(SettingsValue::Lightmaps),
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Display: the window mode and, while windowed, the resolution.
fn display_rows(settings: &Settings, display: &DisplayStatus) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Display.item_count(false));
    push_row(
        &mut rows,
        "Window Mode",
        selector(settings.window_mode().label()),
        SettingsRowKind::Value(SettingsValue::WindowMode),
    );
    push_row(
        &mut rows,
        "Resolution",
        selector(&resolution_label(settings, *display)),
        SettingsRowKind::Value(SettingsValue::Resolution),
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Audio: independent volume levels and a mute switch that preserves them.
fn audio_rows(settings: &Settings) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Audio.item_count(false));
    for (label, volume, value) in [
        (
            "Master Volume",
            settings.master_volume,
            SettingsValue::MasterVolume,
        ),
        (
            "Effects Volume",
            settings.effects_volume,
            SettingsValue::EffectsVolume,
        ),
        (
            "Ambience Volume",
            settings.ambient_volume,
            SettingsValue::AmbientVolume,
        ),
    ] {
        push_row(
            &mut rows,
            label,
            selector(&format!("{:.0}%", volume * 100.0)),
            SettingsRowKind::Value(value),
        );
    }
    push_row(
        &mut rows,
        "Mute",
        selector(on_off(settings.muted)),
        SettingsRowKind::Value(SettingsValue::Mute),
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Controls: every bindable action, the control preferences and Back.
fn controls_rows(settings: &Settings) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Controls.item_count(false));
    for action in KeyBindings::ACTIONS {
        let key = settings.bindings.get_key(action).unwrap_or("");
        push_row(
            &mut rows,
            crate::game::settings::action_label(action).as_str(),
            key.to_string(),
            SettingsRowKind::Binding(action),
        );
    }
    push_row(
        &mut rows,
        "Mouse sensitivity",
        selector(&format!("{:.2} deg/px", settings.mouse_sensitivity)),
        SettingsRowKind::Value(SettingsValue::MouseSensitivity),
    );
    push_row(
        &mut rows,
        "Look Speed H",
        selector(&format!("{:.0} deg/s", settings.look_speed_h)),
        SettingsRowKind::Value(SettingsValue::LookSpeedH),
    );
    push_row(
        &mut rows,
        "Look Speed V",
        selector(&format!("{:.0} deg/s", settings.look_speed_v)),
        SettingsRowKind::Value(SettingsValue::LookSpeedV),
    );
    push_row(
        &mut rows,
        "Invert Look",
        selector(on_off(settings.invert_look)),
        SettingsRowKind::Value(SettingsValue::InvertLook),
    );
    push_row(
        &mut rows,
        "Walk Speed",
        selector(&format!("{:.1} m/s", settings.walk_speed)),
        SettingsRowKind::Value(SettingsValue::WalkSpeed),
    );
    push_row(
        &mut rows,
        "Field of View",
        selector(&format!("{:.0} deg", settings.fov_degrees)),
        SettingsRowKind::Value(SettingsValue::Fov),
    );
    push_row(
        &mut rows,
        "Developer Console",
        selector(on_off(settings.developer_console)),
        SettingsRowKind::Value(SettingsValue::DeveloperConsole),
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Cycles, toggles or rebinds the selected settings row.
///
/// `direction` is a left/right step (`-1`/`1`); `0` means Enter/Activate, which
/// steps a selector forward, toggles the Advanced group, starts a rebind or
/// opens a section. Persisting the change is the caller's job (as is applying
/// it to the running systems), so this stays a pure mutation of the
/// authoritative settings. Returns what the screen should do next.
///
/// The Advanced group is deliberately Enter-only: a left/right step over it is
/// ignored.
pub fn activate_settings_item(
    page: SettingsPage,
    idx: usize,
    ui_state: &mut UiState,
    settings: &mut Settings,
    display: &DisplayStatus,
    direction: i32,
) -> SettingsAction {
    let rows = settings_rows(page, settings, display, ui_state.advanced_expanded);
    let Some(row) = rows.get(idx) else {
        return SettingsAction::None;
    };
    match &row.kind {
        SettingsRowKind::Binding(action) => {
            ui_state.rebinding_action = Some(action);
            ui_state.clear_status();
            SettingsAction::None
        }
        SettingsRowKind::Open(section) => SettingsAction::Open(*section),
        SettingsRowKind::RestoreDefaults => {
            settings.restore_defaults();
            ui_state.cancel_rebinding();
            ui_state.set_status("Restored default settings", false);
            SettingsAction::None
        }
        SettingsRowKind::Back => SettingsAction::Back,
        SettingsRowKind::Advanced => {
            if direction == 0 {
                ui_state.advanced_expanded = !ui_state.advanced_expanded;
                // Collapsing hides the rows after the header; keep the
                // selection on a row that still exists.
                let count = page.item_count(ui_state.advanced_expanded);
                ui_state.settings_idx = ui_state.settings_idx.min(count.saturating_sub(1));
            }
            SettingsAction::None
        }
        SettingsRowKind::Value(value) => {
            let step = if direction == 0 { 1 } else { direction };
            adjust_value(*value, ui_state, settings, display, step);
            SettingsAction::None
        }
    }
}

/// Activates the selected settings row with Enter.
///
/// This is the action path behind `MenuNavEvent::Activate`; it is exactly
/// [`activate_settings_item`] with the explicit "activate, not step" direction,
/// so the Advanced group toggles while left/right over it stays inert.
pub fn activate_settings_row(
    page: SettingsPage,
    idx: usize,
    ui_state: &mut UiState,
    settings: &mut Settings,
    display: &DisplayStatus,
) -> SettingsAction {
    activate_settings_item(page, idx, ui_state, settings, display, 0)
}

/// Applies one left/right or Enter step to an adjustable option.
fn adjust_value(
    value: SettingsValue,
    ui_state: &mut UiState,
    settings: &mut Settings,
    display: &DisplayStatus,
    direction: i32,
) {
    match value {
        SettingsValue::GraphicsQuality => {
            let next = Settings::quality_step(settings.quality_level(), direction);
            // A re-selection of the value already stored is not a value
            // change, but it is still an explicit request: it must repair a
            // renderer that holds a different configuration after a failed or
            // cancelled transition, so record the apply either way.
            if !settings.set_quality(next) {
                settings.request_graphics_reapply();
            }
            ui_state.set_status(format!("Graphics quality: {}", next.label()), false);
        }
        SettingsValue::Bloom => {
            let next = settings.toggle_bloom();
            ui_state.set_status(format!("Bloom {}", on_off(next).to_lowercase()), false);
        }
        SettingsValue::Reflections => {
            if !settings.lightmaps_enabled() {
                ui_state.set_status("Select Full lighting to change reflections", false);
                return;
            }
            let next = Settings::reflection_quality_step(settings.reflection_quality(), direction);
            if settings.set_reflection_quality(next) {
                ui_state.set_status(format!("Reflections: {}", next.label()), false);
            }
        }
        SettingsValue::Lightmaps => adjust_lightmaps(ui_state, settings, direction),
        SettingsValue::Vsync => {
            let next = settings.toggle_vsync();
            ui_state.set_status(format!("VSync {}", on_off(next).to_lowercase()), false);
        }
        SettingsValue::Filtering => {
            let next = crate::game::settings::texture_filtering_step(
                settings.texture_filtering_preset(),
                direction,
            );
            if settings.set_texture_filtering(next) {
                ui_state.set_status(
                    format!("Texture filtering: {}", filtering_label(next)),
                    false,
                );
            }
        }
        SettingsValue::WindowMode => {
            let mode = settings.step_window_mode(direction);
            ui_state.set_status(format!("Window mode: {}", mode.label()), false);
        }
        SettingsValue::Resolution => adjust_resolution(ui_state, settings, display, direction),
        SettingsValue::MasterVolume => {
            let next = step_volume(settings.master_volume, direction);
            let _changed = settings.set_master_volume(next);
            ui_state.set_status(format!("Master volume: {:.0}%", next * 100.0), false);
        }
        SettingsValue::EffectsVolume => {
            let next = step_volume(settings.effects_volume, direction);
            let _changed = settings.set_effects_volume(next);
            ui_state.set_status(format!("Effects volume: {:.0}%", next * 100.0), false);
        }
        SettingsValue::AmbientVolume => {
            let next = step_volume(settings.ambient_volume, direction);
            let _changed = settings.set_ambient_volume(next);
            ui_state.set_status(format!("Ambience volume: {:.0}%", next * 100.0), false);
        }
        SettingsValue::Mute => adjust_mute(ui_state, settings),
        SettingsValue::LookSpeedH => {
            step_range(&mut settings.look_speed_h, direction, 15.0, 45.0, 180.0);
            ui_state.set_status("Horizontal look speed updated", false);
        }
        SettingsValue::LookSpeedV => {
            step_range(&mut settings.look_speed_v, direction, 15.0, 30.0, 150.0);
            ui_state.set_status("Vertical look speed updated", false);
        }
        SettingsValue::DeveloperConsole => {
            settings.developer_console = !settings.developer_console;
            ui_state.set_status("Developer console: backtick opens during play", false);
        }
        SettingsValue::InvertLook => {
            settings.set_invert_look(!settings.invert_look);
            ui_state.set_status(
                format!("Invert vertical look: {}", on_off(settings.invert_look)),
                false,
            );
        }
        SettingsValue::MouseSensitivity => {
            step_range(
                &mut settings.mouse_sensitivity,
                direction,
                MOUSE_SENSITIVITY_STEP,
                crate::game::settings::MIN_MOUSE_SENSITIVITY,
                crate::game::settings::MAX_MOUSE_SENSITIVITY,
            );
            ui_state.set_status("Mouse sensitivity updated", false);
        }
        SettingsValue::WalkSpeed => {
            step_range(&mut settings.walk_speed, direction, 0.5, 1.5, 6.0);
            ui_state.set_status("Walk speed updated", false);
        }
        SettingsValue::Fov => {
            step_range(&mut settings.fov_degrees, direction, 15.0, 45.0, 90.0);
            ui_state.set_status("Field of view updated", false);
        }
    }
}

/// Selects the lighting path without changing texture preferences.
fn adjust_lightmaps(ui_state: &mut UiState, settings: &mut Settings, direction: i32) {
    let next = Settings::lightmap_quality_step(settings.lightmap_quality(), direction);
    if settings.set_lightmap_quality(next) {
        ui_state.set_status(format!("Lighting: {}", next.label()), false);
    }
}

/// Mouse sensitivity step per left/right input, in degrees per pixel.
const MOUSE_SENSITIVITY_STEP: f32 = 0.02;

/// Changes volume by five percentage points without wrapping to the other end.
fn step_volume(volume: f32, direction: i32) -> f32 {
    let step = if direction < 0_i32 { -0.05 } else { 0.05 };
    (volume + step).clamp(0.0, 1.0)
}

/// Moves a scalar value one step and wraps it around `min`..=`max`.
fn step_range(value: &mut f32, direction: i32, step: f32, min: f32, max: f32) {
    *value += if direction < 0_i32 { -step } else { step };
    if *value > max {
        *value = min;
    } else if *value < min {
        *value = max;
    }
}

fn adjust_resolution(
    ui_state: &mut UiState,
    settings: &mut Settings,
    display: &DisplayStatus,
    direction: i32,
) {
    if display.mode == WindowMode::Fullscreen {
        ui_state.set_status("Resolution follows the display in fullscreen mode", false);
    } else {
        let choices = display.resolution_choices(settings.window_size());
        if let Some(next) = step_resolution(&choices, settings.window_size(), direction)
            && settings.set_window_size(next.0, next.1)
        {
            ui_state.set_status(
                format!(
                    "Resolution: {}",
                    crate::game::display::format_resolution(next)
                ),
                false,
            );
        }
    }
}

fn adjust_mute(ui_state: &mut UiState, settings: &mut Settings) {
    let _changed = settings.set_muted(!settings.muted);
    ui_state.set_status(
        if settings.muted {
            "Audio muted"
        } else {
            "Audio unmuted"
        },
        false,
    );
}

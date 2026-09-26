use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::display::{DisplayStatus, resolution_label, step_resolution};
use crate::font::{get_char_uv, get_white_uv};
use crate::game::AppState;
use crate::quality::QualityProfile;
use crate::render::Vertex;
use crate::settings::{KeyBindings, Settings, WindowMode};

/// Which Settings screen is open.
///
/// The root is a table of contents; the three sections group the actual
/// options. Both entry points (the main menu's Settings and the pause menu's
/// Settings) share this screen, so the page is state, not a separate
/// `AppState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    /// Graphics / Display / Controls, Restore Defaults and Back.
    #[default]
    Root,
    /// Rendering and visual-quality options.
    Graphics,
    /// Window mode and resolution.
    Display,
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
            Self::Controls => "CONTROLS",
        }
    }

    /// Number of selectable rows on this page.
    ///
    /// Kept next to [`settings_rows`], which generates exactly this many rows;
    /// a unit test pins the two together.
    ///
    /// Root and Graphics happen to declare the same count; they are different
    /// pages and the arms stay separate so a row added to one does not silently
    /// change the other.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn item_count(self) -> usize {
        match self {
            Self::Root => 5,
            Self::Graphics => 5,
            Self::Display => 3,
            Self::Controls => 14,
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
    Lightmaps,
    Vsync,
    Filtering,
    WindowMode,
    Resolution,
    LookSpeedH,
    LookSpeedV,
    InvertLook,
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
    pub rebinding_action: Option<&'static str>,
    pub status_message: Option<String>,
    /// True when [`Self::status_message`] describes a failure, so screens can
    /// colour it without parsing their own text.
    pub status_is_error: bool,
    pub level_entries: Vec<String>,
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
    (app_state as u8).hash(&mut hasher);
    ui_state.main_menu_idx.hash(&mut hasher);
    ui_state.level_select_idx.hash(&mut hasher);
    ui_state.pause_menu_idx.hash(&mut hasher);
    ui_state.settings_idx.hash(&mut hasher);
    ui_state.settings_page.hash(&mut hasher);
    ui_state.rebinding_action.hash(&mut hasher);
    ui_state.status_message.as_deref().hash(&mut hasher);
    ui_state.status_is_error.hash(&mut hasher);
    ui_state.level_entries.hash(&mut hasher);
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
    settings.look_speed_h.to_bits().hash(&mut hasher);
    settings.look_speed_v.to_bits().hash(&mut hasher);
    settings.walk_speed.to_bits().hash(&mut hasher);
    settings.fov_degrees.to_bits().hash(&mut hasher);
    settings.invert_look.hash(&mut hasher);
    // The *effective* values: a startup override changes what the screen says.
    settings.vsync_enabled().hash(&mut hasher);
    settings.lightmaps_enabled().hash(&mut hasher);
    settings.quality_profile().hash(&mut hasher);
    settings.window_mode().hash(&mut hasher);
    settings.texture_filtering.hash(&mut hasher);
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

/// Horizontal x that centres `text` at `scale` inside `[left, right]`.
#[must_use]
fn centered_x(text: &str, scale: f32, left: f32, right: f32) -> f32 {
    let offset = (right - left - text_width(text, scale)).mul_add(0.5, left);
    offset.max(left)
}

/// Draws one centred legend line inside the panel bounds.
fn draw_legend(vertices: &mut Vec<Vertex>, text: &str, left: f32, right: f32, y: f32) {
    let x = centered_x(text, 1.0, left, right);
    draw_text(vertices, text, x, y, 1.0, [0.55, 0.55, 0.50]);
}

/// Generates 2D UI overlay vertices in the 480x272 reference space for the
/// active `AppState`. `Renderer::render_ui` scales this space to the drawable.
#[must_use]
pub fn build_ui_geometry(
    app_state: AppState,
    ui_state: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
    version: &str,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();

    match app_state {
        AppState::Playing => {
            // No full-screen menu; optional version or overlay if needed
        }
        AppState::MainMenu => main_menu_geometry(&mut vertices, ui_state, version),
        AppState::LevelSelect => level_select_geometry(&mut vertices, ui_state),
        AppState::Paused => pause_menu_geometry(&mut vertices, ui_state),
        AppState::Settings | AppState::PauseSettings => {
            settings_geometry(&mut vertices, ui_state, settings, display);
        }
    }

    vertices
}

/// Y offset of menu row `index` in the 480x272 reference space.
///
/// Menus hold at most a few dozen rows, so the `u16` conversion is exact and
/// the row offset is lossless.
fn row_y(index: usize, line_h: f32, start_y: f32) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(u16::MAX)).mul_add(line_h, start_y)
}

/// Main menu: scrim, title, three items and the control legend.
fn main_menu_geometry(vertices: &mut Vec<Vertex>, ui_state: &UiState, version: &str) {
    // Partially transparent dark scrim background panel (70% opacity)
    let opacity = 0.70;
    // Border outline strips (non-overlapping with inner panel)
    add_rect_rgba(
        vertices,
        20.0,
        20.0,
        460.0,
        22.0,
        [0.08, 0.08, 0.07, opacity],
    );
    add_rect_rgba(
        vertices,
        20.0,
        250.0,
        460.0,
        252.0,
        [0.08, 0.08, 0.07, opacity],
    );
    add_rect_rgba(
        vertices,
        20.0,
        22.0,
        22.0,
        250.0,
        [0.08, 0.08, 0.07, opacity],
    );
    add_rect_rgba(
        vertices,
        458.0,
        22.0,
        460.0,
        250.0,
        [0.08, 0.08, 0.07, opacity],
    );
    // Inner panel
    add_rect_rgba(
        vertices,
        22.0,
        22.0,
        458.0,
        250.0,
        [0.12, 0.11, 0.10, opacity],
    );

    // Title
    draw_text(
        vertices,
        "Places",
        centered_x("Places", 2.0, 22.0, 458.0),
        36.0,
        2.0,
        [0.92, 0.88, 0.45],
    );
    draw_text(
        vertices,
        "an experience",
        centered_x("an experience", 1.0, 22.0, 458.0),
        58.0,
        1.0,
        [0.65, 0.65, 0.60],
    );

    // Menu Items
    let items = ["Level Select", "Settings", "Exit"];
    let start_y = 95.0;
    let line_h = 24.0;

    for (i, &item) in items.iter().enumerate() {
        let y = row_y(i, line_h, start_y);
        let is_sel = i == ui_state.main_menu_idx;

        if is_sel {
            add_rect(vertices, 38.0, y - 2.0, 260.0, y + 14.0, [0.25, 0.23, 0.16]);
            let line = format!("> {item}");
            draw_text(vertices, &line, 40.0, y, 1.0, [1.0, 0.95, 0.40]);
        } else {
            let line = format!("  {item}");
            draw_text(vertices, &line, 40.0, y, 1.0, [0.85, 0.85, 0.80]);
        }
    }

    // Version bottom-left, aligned with the menu items above it.
    let ver_text = format!("v{version}");
    draw_text(vertices, &ver_text, 40.0, 235.0, 1.0, [0.5, 0.5, 0.5]);

    // Controls help bottom.
    draw_legend(vertices, "W/S: Move   ENTER: Select", 22.0, 458.0, 235.0);
}

/// Level selection: scrolling list of installed levels, status line and legend.
fn level_select_geometry(vertices: &mut Vec<Vertex>, ui_state: &UiState) {
    add_rect(vertices, 20.0, 20.0, 460.0, 252.0, [0.08, 0.08, 0.07]);
    add_rect(vertices, 22.0, 22.0, 458.0, 250.0, [0.12, 0.11, 0.10]);

    draw_text(
        vertices,
        "LEVEL SELECT",
        centered_x("LEVEL SELECT", 2.0, 22.0, 458.0),
        38.0,
        2.0,
        [0.92, 0.88, 0.45],
    );

    level_select_items(vertices, ui_state);

    if ui_state.level_entries.is_empty() {
        draw_text(
            vertices,
            "No level files installed - import one below.",
            40.0,
            190.0,
            1.0,
            [0.6, 0.6, 0.55],
        );
    }

    if let Some(ref msg) = ui_state.status_message {
        let col = if ui_state.status_is_error {
            [1.0, 0.4, 0.3]
        } else {
            [0.4, 0.9, 0.4]
        };
        // The list panel is 440 px wide; a loader diagnostic can be longer.
        let line = fit_text(msg, 420.0, 1.0);
        draw_text(vertices, &line, 40.0, 212.0, 1.0, col);
    }

    draw_legend(
        vertices,
        "W/S: Move   ENTER: Select   ESC: Back",
        22.0,
        458.0,
        235.0,
    );
}

/// The scrolling level list body: at most six rows plus the trailing actions.
///
/// An empty list (no installed demo and no drop-in levels) shows only the
/// trailing actions, so selection, drawing and activation all agree on the
/// same row indices.
fn level_select_items(vertices: &mut Vec<Vertex>, ui_state: &UiState) {
    let mut items: Vec<String> = ui_state.level_entries.clone();
    items.push("Import Levels".to_string());
    items.push("Back".to_string());

    // Show at most 6 items per page with scrolling
    let max_visible = 6;
    let total = items.len();
    let scroll_offset = if total <= max_visible || ui_state.level_select_idx < max_visible {
        0
    } else if ui_state.level_select_idx >= total.saturating_sub(max_visible) {
        total.saturating_sub(max_visible)
    } else {
        ui_state
            .level_select_idx
            .saturating_sub(max_visible.saturating_sub(1))
    };

    let start_y = 75.0;
    let line_h = 22.0;

    for (vi, (i, label)) in items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(max_visible)
        .enumerate()
    {
        let y = row_y(vi, line_h, start_y);
        let is_sel = i == ui_state.level_select_idx;
        // `> ` or `  ` plus the row indent, then the label; a long name is
        // shortened so it cannot run past the panel.
        let label = fit_text(label, 320.0, 1.0);

        if is_sel {
            add_rect(vertices, 38.0, y - 2.0, 380.0, y + 14.0, [0.25, 0.23, 0.16]);
            let line = format!("> {label}");
            draw_text(vertices, &line, 40.0, y, 1.0, [1.0, 0.95, 0.40]);
        } else {
            let line = format!("  {label}");
            draw_text(vertices, &line, 40.0, y, 1.0, [0.85, 0.85, 0.80]);
        }
    }
}

/// Pause menu: panel, title, three items and the legend.
fn pause_menu_geometry(vertices: &mut Vec<Vertex>, ui_state: &UiState) {
    // Opaque pause panel over the frozen scene.
    add_rect(vertices, 80.0, 40.0, 400.0, 230.0, [0.06, 0.06, 0.05]);
    add_rect(vertices, 82.0, 42.0, 398.0, 228.0, [0.12, 0.11, 0.10]);

    draw_text(
        vertices,
        "PAUSED",
        centered_x("PAUSED", 2.0, 82.0, 398.0),
        58.0,
        2.0,
        [0.92, 0.88, 0.45],
    );

    let items = ["Resume", "Settings", "Return to Main Menu"];
    let start_y = 105.0;
    let line_h = 24.0;

    for (i, &item) in items.iter().enumerate() {
        let y = row_y(i, line_h, start_y);
        let is_sel = i == ui_state.pause_menu_idx;

        if is_sel {
            add_rect(vertices, 98.0, y - 2.0, 340.0, y + 14.0, [0.25, 0.23, 0.16]);
            let line = format!("> {item}");
            draw_text(vertices, &line, 100.0, y, 1.0, [1.0, 0.95, 0.40]);
        } else {
            let line = format!("  {item}");
            draw_text(vertices, &line, 100.0, y, 1.0, [0.85, 0.85, 0.80]);
        }
    }

    draw_legend(
        vertices,
        "W/S: Move   ENTER: Select   ESC: Resume",
        82.0,
        398.0,
        205.0,
    );
}

/// The three section entries the Settings root offers, in menu order.
pub const SETTINGS_SECTIONS: [SettingsPage; 3] = [
    SettingsPage::Graphics,
    SettingsPage::Display,
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
            Self::Controls => "Controls",
        }
    }
}

/// Suffix appended to a value pinned by a startup override.
const OVERRIDE_MARKER: &str = " *";

/// Horizontal x the value column is right-aligned to, in the reference space.
const VALUE_RIGHT: f32 = 450.0;
/// Left x the row text starts at.
const ROW_LEFT: f32 = 25.0;

const fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

const fn profile_label(profile: QualityProfile) -> &'static str {
    match profile {
        QualityProfile::Full => "Full",
        QualityProfile::Low => "Low",
    }
}

// `==` on `&str` is not const-callable yet, so this cannot be `const fn`;
// clippy's suggestion is a known false positive for this case.
#[allow(clippy::missing_const_for_fn)]
fn filtering_label(mode: &str) -> &'static str {
    if mode == "nearest" {
        "Nearest"
    } else {
        "Linear"
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
pub const fn any_graphics_override(settings: &Settings) -> bool {
    settings.quality_overridden() || settings.lightmaps_overridden() || settings.vsync_overridden()
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
#[must_use]
pub fn settings_rows(
    page: SettingsPage,
    settings: &Settings,
    display: &DisplayStatus,
) -> Vec<SettingsRow> {
    match page {
        SettingsPage::Root => root_rows(),
        SettingsPage::Graphics => graphics_rows(settings),
        SettingsPage::Display => display_rows(settings, display),
        SettingsPage::Controls => controls_rows(settings),
    }
}

/// The Settings root: one entry per section, Restore Defaults and Back.
fn root_rows() -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Root.item_count());
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

/// Graphics: the quality profile, then the independent visual toggles.
fn graphics_rows(settings: &Settings) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Graphics.item_count());
    push_row(
        &mut rows,
        "Graphics Quality",
        selector_with_override(
            profile_label(settings.quality_profile()),
            settings.quality_overridden(),
        ),
        SettingsRowKind::Value(SettingsValue::GraphicsQuality),
    );
    push_row(
        &mut rows,
        "Lightmaps",
        selector_with_override(
            on_off(settings.lightmaps_enabled()),
            settings.lightmaps_overridden(),
        ),
        SettingsRowKind::Value(SettingsValue::Lightmaps),
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
        "Texture Filtering",
        selector(filtering_label(&settings.texture_filtering)),
        SettingsRowKind::Value(SettingsValue::Filtering),
    );
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Display: the window mode and, while windowed, the resolution.
fn display_rows(settings: &Settings, display: &DisplayStatus) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Display.item_count());
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

/// Controls: every bindable action, the control preferences and Back.
fn controls_rows(settings: &Settings) -> Vec<SettingsRow> {
    let mut rows = Vec::with_capacity(SettingsPage::Controls.item_count());
    for action in KeyBindings::ACTIONS {
        let key = settings.bindings.get_key(action).unwrap_or("");
        push_row(
            &mut rows,
            crate::settings::action_label(action).as_str(),
            key.to_string(),
            SettingsRowKind::Binding(action),
        );
    }
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
    push_row(&mut rows, "Back", String::new(), SettingsRowKind::Back);
    rows
}

/// Settings screen: panel, rebinding/status prompt, page rows and legend.
fn settings_geometry(
    vertices: &mut Vec<Vertex>,
    ui_state: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) {
    add_rect(vertices, 10.0, 10.0, 470.0, 262.0, [0.08, 0.08, 0.07]);
    add_rect(vertices, 12.0, 12.0, 468.0, 260.0, [0.12, 0.11, 0.10]);

    let page = ui_state.settings_page;
    draw_text(vertices, page.title(), 25.0, 20.0, 2.0, [0.92, 0.88, 0.45]);

    // Rebinding prompt or status message.
    if let Some(action) = ui_state.rebinding_action {
        add_rect(vertices, 22.0, 18.0, 462.0, 32.0, [0.35, 0.15, 0.10]);
        let label = crate::settings::action_label(action);
        let prompt = fit_text(&format!("PRESS KEY FOR {label} (ESC: CANCEL)"), 432.0, 1.0);
        draw_text(vertices, &prompt, 26.0, 21.0, 1.0, [1.0, 0.9, 0.3]);
    } else if let Some(ref msg) = ui_state.status_message {
        let col = if ui_state.status_is_error {
            [1.0, 0.4, 0.3]
        } else {
            [0.4, 0.9, 0.4]
        };
        let line = fit_text(msg, 320.0, 1.0);
        draw_text(vertices, &line, 160.0, 22.0, 1.0, col);
    }

    settings_item_rows(vertices, ui_state, settings, display);

    let legend = match page {
        SettingsPage::Root => "W/S: Move   ENTER: Open   ESC: Back",
        SettingsPage::Graphics | SettingsPage::Display => "A/D: Change   ENTER: Apply   ESC: Back",
        SettingsPage::Controls => "ENTER: Rebind   A/D: Change   ESC: Back",
    };
    draw_legend(vertices, legend, 22.0, 458.0, 250.0);

    // A footnote line the rows have room for: where the graphics values come
    // from when a startup override pinned them, or which controls are fixed.
    let footnote = match page {
        SettingsPage::Graphics if any_graphics_override(settings) => "* = startup override",
        SettingsPage::Root | SettingsPage::Graphics | SettingsPage::Display => "",
        SettingsPage::Controls => "Fixed: ESC Pause   W/S/A/D Menu   - Overlay",
    };
    if !footnote.is_empty() {
        draw_legend(vertices, footnote, 22.0, 458.0, 238.0);
    }
}

/// One row per settings item, showing the current binding or value.
///
/// Labels are left-aligned and values right-aligned; a label long enough to
/// reach the value column pushes the value right instead of overlapping it.
fn settings_item_rows(
    vertices: &mut Vec<Vertex>,
    ui_state: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) {
    let rows = settings_rows(ui_state.settings_page, settings, display);
    let start_y = 44.0;
    let line_h = 13.0;

    for (i, row) in rows.iter().enumerate() {
        let y = row_y(i, line_h, start_y);
        let is_sel = i == ui_state.settings_idx;
        let label_color = if is_sel {
            [1.0, 0.95, 0.40]
        } else {
            [0.85, 0.85, 0.80]
        };
        let value_color = if is_sel {
            [1.0, 0.95, 0.40]
        } else {
            [0.62, 0.70, 0.72]
        };

        if is_sel {
            add_rect(
                vertices,
                23.0,
                y - 1.0,
                VALUE_RIGHT,
                y + 10.0,
                [0.25, 0.23, 0.16],
            );
        }
        let prefix = if is_sel { "> " } else { "  " };
        let text = format!("{prefix}{}", row.label);
        draw_text(vertices, &text, ROW_LEFT, y, 1.0, label_color);
        if !row.value.is_empty() {
            let value_x = (VALUE_RIGHT - text_width(&row.value, 1.0))
                .max(ROW_LEFT + text_width(&text, 1.0) + 8.0);
            draw_text(vertices, &row.value, value_x, y, 1.0, value_color);
        }
    }
}

/// Cycles, toggles or rebinds the selected settings row.
///
/// Persisting the change is the caller's job (as is applying it to the running
/// systems), so this stays a pure mutation of the authoritative settings.
/// Returns what the screen should do next.
pub fn activate_settings_item(
    page: SettingsPage,
    idx: usize,
    ui_state: &mut UiState,
    settings: &mut Settings,
    display: &DisplayStatus,
    direction: i32,
) -> SettingsAction {
    let rows = settings_rows(page, settings, display);
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
        SettingsRowKind::Value(value) => {
            adjust_value(*value, ui_state, settings, display, direction);
            SettingsAction::None
        }
    }
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
            let next = Settings::quality_step(settings.quality_profile(), direction);
            if settings.set_quality(next) {
                ui_state.set_status(format!("Graphics quality: {}", profile_label(next)), false);
            }
        }
        SettingsValue::Lightmaps => {
            let next = settings.toggle_lightmaps();
            ui_state.set_status(format!("Lightmaps {}", on_off(next).to_lowercase()), false);
        }
        SettingsValue::Vsync => {
            let next = settings.toggle_vsync();
            ui_state.set_status(format!("VSync {}", on_off(next).to_lowercase()), false);
        }
        SettingsValue::Filtering => {
            settings.texture_filtering = if settings.texture_filtering == "linear" {
                "nearest".to_string()
            } else {
                "linear".to_string()
            };
            ui_state.set_status(
                format!(
                    "Texture filtering: {}",
                    filtering_label(&settings.texture_filtering)
                ),
                false,
            );
        }
        SettingsValue::WindowMode => {
            let mode = settings.step_window_mode(direction);
            ui_state.set_status(format!("Window mode: {}", mode.label()), false);
        }
        SettingsValue::Resolution => {
            if display.mode == WindowMode::Fullscreen {
                ui_state.set_status("Resolution follows the display in fullscreen mode", false);
            } else {
                let choices = display.resolution_choices(settings.window_size());
                if let Some(next) = step_resolution(&choices, settings.window_size(), direction)
                    && settings.set_window_size(next.0, next.1)
                {
                    ui_state.set_status(
                        format!("Resolution: {}", crate::display::format_resolution(next)),
                        false,
                    );
                }
            }
        }
        SettingsValue::LookSpeedH => {
            step_range(&mut settings.look_speed_h, direction, 15.0, 45.0, 180.0);
            ui_state.set_status("Horizontal look speed updated", false);
        }
        SettingsValue::LookSpeedV => {
            step_range(&mut settings.look_speed_v, direction, 15.0, 30.0, 150.0);
            ui_state.set_status("Vertical look speed updated", false);
        }
        SettingsValue::InvertLook => {
            settings.set_invert_look(!settings.invert_look);
            ui_state.set_status(
                format!("Invert vertical look: {}", on_off(settings.invert_look)),
                false,
            );
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

/// Moves a scalar value one step and wraps it around `min`..=`max`.
fn step_range(value: &mut f32, direction: i32, step: f32, min: f32, max: f32) {
    *value += if direction < 0 { -step } else { step };
    if *value > max {
        *value = min;
    } else if *value < min {
        *value = max;
    }
}

#[cfg(test)]
mod tests {
    // Test code: `expect` documents the invariant being asserted and row
    // indexing documents which row a page declares; the production lints stay
    // enforced everywhere else in the crate.
    #![allow(clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::settings::SettingsApply;
    use crate::test_support::assert_exact;

    #[test]
    fn text_width_counts_every_glyph_including_spaces_and_unknowns() {
        assert_exact(text_width("AB", 1.0), 16.0);
        assert_exact(text_width("A B", 1.0), 24.0);
        assert_exact(text_width("AB", 2.0), 32.0);
        // An unsupported character advances without drawing, exactly like the
        // renderer does, so it still occupies width.
        assert_exact(text_width("é", 1.0), 8.0);
    }

    #[test]
    fn fit_text_truncates_without_exceeding_its_budget() {
        let long = "A".repeat(100);
        let fitted = fit_text(&long, 80.0, 1.0);
        assert_eq!(fitted.chars().count(), 10);
        assert!(fitted.ends_with("..."));
        assert!(text_width(&fitted, 1.0) <= 80.0);

        // Short text and an exact fit are untouched.
        assert_eq!(fit_text("ok", 80.0, 1.0), "ok");
        assert_eq!(fit_text("1234567890", 80.0, 1.0), "1234567890");
    }

    #[test]
    fn the_rebind_prompt_fits_the_panel_for_every_action() {
        // The prompt is drawn at scale 1 inside a 432 px box; the longest
        // action label ("Strafe Right") must not overflow it.
        for action in KeyBindings::ACTIONS {
            let label = crate::settings::action_label(action);
            let prompt = fit_text(&format!("PRESS KEY FOR {label} (ESC: CANCEL)"), 432.0, 1.0);
            assert!(
                text_width(&prompt, 1.0) <= 432.0,
                "prompt for {action} is too wide: {prompt:?}"
            );
        }
    }

    #[test]
    fn status_messages_carry_their_own_error_flag() {
        let mut ui = UiState::new();
        assert!(ui.status_message.is_none());
        assert!(!ui.status_is_error);

        ui.set_status("Load failed: nothing", true);
        assert!(ui.status_is_error);

        ui.set_status("Imported 1 level", false);
        assert!(!ui.status_is_error);
        assert_eq!(ui.status_message.as_deref(), Some("Imported 1 level"));

        ui.clear_status();
        assert!(ui.status_message.is_none());
        assert!(!ui.status_is_error);

        ui.set_status("bound", false);
        ui.cancel_rebinding();
        assert!(ui.status_message.is_none(), "cancel clears the message too");
    }

    #[test]
    fn every_page_generates_exactly_the_rows_it_declares() {
        let settings = Settings::default();
        let display = DisplayStatus::default();
        for page in [
            SettingsPage::Root,
            SettingsPage::Graphics,
            SettingsPage::Display,
            SettingsPage::Controls,
        ] {
            assert_eq!(
                settings_rows(page, &settings, &display).len(),
                page.item_count(),
                "{page:?} row count"
            );
        }
    }

    #[test]
    fn every_settings_row_activates_without_breaking_the_settings() {
        // Walk every page in both directions, including the binding rows
        // (which open a rebind), the value rows (which wrap), the section rows
        // and Back. Back must be the only row that signals leaving.
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        let display = DisplayStatus::default();
        for page in [
            SettingsPage::Root,
            SettingsPage::Graphics,
            SettingsPage::Display,
            SettingsPage::Controls,
        ] {
            for idx in 0..page.item_count() {
                for direction in [1, -1] {
                    let action = activate_settings_item(
                        page,
                        idx,
                        &mut ui,
                        &mut settings,
                        &display,
                        direction,
                    );
                    assert_eq!(
                        action == SettingsAction::Back,
                        idx == page.item_count() - 1,
                        "page {page:?} row {idx} direction {direction} action {action:?}"
                    );
                    ui.cancel_rebinding();
                }
            }
        }
        settings.sanitize();
        for action in KeyBindings::ACTIONS {
            assert!(settings.bindings.get_key(action).is_some());
        }
    }

    #[test]
    fn restore_defaults_resets_every_persisted_preference_and_requests_every_apply() {
        let mut ui = UiState::new();
        let mut settings = Settings {
            look_speed_h: 180.0,
            look_speed_v: 20.0,
            walk_speed: 6.0,
            fov_degrees: 90.0,
            invert_look: true,
            vsync: false,
            texture_filtering: "nearest".to_string(),
            quality: "low".to_string(),
            lightmaps: false,
            window_mode: "fullscreen".to_string(),
            window_width: 1280,
            window_height: 720,
            ..Settings::default()
        };
        let display = DisplayStatus::default();
        let action =
            activate_settings_item(SettingsPage::Root, 3, &mut ui, &mut settings, &display, 1);
        assert_eq!(action, SettingsAction::None);
        assert_eq!(
            settings.take_pending_apply(),
            SettingsApply::ALL,
            "a restore must ask every subsystem to re-read the defaults"
        );
        assert_eq!(settings, Settings::default());
        assert_eq!(
            ui.status_message.as_deref(),
            Some("Restored default settings")
        );
    }

    #[test]
    fn the_quality_selector_steps_off_the_shipping_profile_and_back() {
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        let display = DisplayStatus::default();
        assert_eq!(settings.quality_profile(), QualityProfile::Low);

        activate_settings_item(
            SettingsPage::Graphics,
            0,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert_eq!(settings.quality_profile(), QualityProfile::Full);
        assert!(
            settings.take_pending_apply().graphics_rebuild,
            "Full must rebuild the GPU resources"
        );

        activate_settings_item(
            SettingsPage::Graphics,
            0,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert_eq!(settings.quality_profile(), QualityProfile::Low);
        assert!(settings.take_pending_apply().graphics_rebuild);

        activate_settings_item(
            SettingsPage::Graphics,
            0,
            &mut ui,
            &mut settings,
            &display,
            -1,
        );
        assert_eq!(
            settings.quality_profile(),
            QualityProfile::Full,
            "left steps back"
        );
    }

    #[test]
    fn the_menu_shows_effective_values_not_stale_saved_ones() {
        let mut settings = Settings {
            quality: "low".to_string(),
            lightmaps: false,
            ..Settings::default()
        };
        // A startup override selects Full + Lightmaps Off for this process.
        settings.overrides.quality = Some(QualityProfile::Full);
        settings.overrides.lightmaps = Some(true);
        let display = DisplayStatus::default();
        let rows = settings_rows(SettingsPage::Graphics, &settings, &display);
        assert_eq!(rows[0].value, "< Full > *");
        assert_eq!(rows[1].value, "< On > *");

        // An explicit change clears the override and persists the new choice.
        let mut ui = UiState::new();
        activate_settings_item(
            SettingsPage::Graphics,
            1,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert!(!settings.lightmaps_enabled());
        assert!(!settings.lightmaps_overridden());
        assert!(
            !settings.lightmaps,
            "the player's choice is the saved value now"
        );
    }

    #[test]
    fn the_controls_page_shows_the_authoritative_bindings() {
        let mut settings = Settings::default();
        settings.bindings.set_key("forward", "I").expect("rebind");
        let display = DisplayStatus::default();
        let rows = settings_rows(SettingsPage::Controls, &settings, &display);
        assert_eq!(rows[0].label, "Forward");
        assert_eq!(rows[0].value, "I", "the row reads the real binding");
        for action in KeyBindings::ACTIONS {
            assert!(
                rows.iter()
                    .any(|row| row.kind == SettingsRowKind::Binding(action)),
                "{action} has no Controls row"
            );
        }
        // Control preferences the game actually supports are present too.
        for value in [
            SettingsValue::LookSpeedH,
            SettingsValue::LookSpeedV,
            SettingsValue::InvertLook,
            SettingsValue::WalkSpeed,
            SettingsValue::Fov,
        ] {
            assert!(
                rows.iter()
                    .any(|row| row.kind == SettingsRowKind::Value(value)),
                "{value:?} has no Controls row"
            );
        }
    }

    #[test]
    fn display_and_vsync_changes_ask_the_window_backend_to_reapply() {
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        // The resolution row steps between the modes that fit the work area, so
        // start from a windowed 16:9 size rather than the handheld's own panel
        // mode: there is exactly one mode on the device and nothing to step to.
        settings.set_window_size(1920, 1080);
        settings.set_window_mode(WindowMode::Windowed);
        let _ = settings.take_pending_apply();
        let display = DisplayStatus {
            mode: WindowMode::Windowed,
            usable_bounds: (2560, 1440),
            ..DisplayStatus::default()
        };

        // VSync: an immediate backend update, not a restart.
        activate_settings_item(
            SettingsPage::Graphics,
            2,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert!(!settings.vsync_enabled());
        assert!(settings.take_pending_apply().vsync);

        // Window mode: one step goes full screen and the next returns.
        activate_settings_item(
            SettingsPage::Display,
            0,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert_eq!(settings.window_mode(), WindowMode::Fullscreen);
        assert!(settings.take_pending_apply().window);
        activate_settings_item(
            SettingsPage::Display,
            0,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert_eq!(settings.window_mode(), WindowMode::Windowed);

        // Resolution steps to the next mode that fits the work area.
        activate_settings_item(
            SettingsPage::Display,
            1,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert_eq!(settings.window_size(), (2560, 1440));
        assert!(settings.take_pending_apply().window);

        // In fullscreen the resolution follows the display and is not edited.
        settings.set_window_mode(WindowMode::Fullscreen);
        let _ = settings.take_pending_apply();
        let fullscreen_display = DisplayStatus {
            mode: WindowMode::Fullscreen,
            ..display
        };
        activate_settings_item(
            SettingsPage::Display,
            1,
            &mut ui,
            &mut settings,
            &fullscreen_display,
            1,
        );
        assert_eq!(settings.window_size(), (2560, 1440));
        assert!(!settings.take_pending_apply().window);
        assert_eq!(
            ui.status_message.as_deref(),
            Some("Resolution follows the display in fullscreen mode")
        );
    }

    #[test]
    fn every_screen_draws_inside_the_reference_frame() {
        // Long names and diagnostics are the clipping risk: they are fitted,
        // so no UI vertex may leave the 480x272 reference space on any screen.
        for state in [
            AppState::MainMenu,
            AppState::LevelSelect,
            AppState::Paused,
            AppState::Settings,
            AppState::PauseSettings,
        ] {
            for page in [
                SettingsPage::Root,
                SettingsPage::Graphics,
                SettingsPage::Display,
                SettingsPage::Controls,
            ] {
                let mut ui = UiState::new();
                ui.settings_page = page;
                ui.level_entries = vec![
                    "A level name long enough to overflow its panel if it were not shortened"
                        .to_string(),
                ];
                ui.set_status(
                    "A status message long enough to run off the right edge if it were not shortened",
                    false,
                );
                let mut settings = Settings::default();
                settings.overrides.quality = Some(QualityProfile::Low);
                let vertices =
                    build_ui_geometry(state, &ui, &settings, &DisplayStatus::default(), "9.9.9");
                assert!(!vertices.is_empty(), "{state:?}/{page:?} drew nothing");
                for vertex in &vertices {
                    assert!(
                        (0.0..=480.0).contains(&vertex.pos[0]),
                        "{state:?}/{page:?} vertex x {} is outside the frame",
                        vertex.pos[0]
                    );
                    assert!(
                        (0.0..=272.0).contains(&vertex.pos[1]),
                        "{state:?}/{page:?} vertex y {} is outside the frame",
                        vertex.pos[1]
                    );
                }
            }
        }
    }

    #[test]
    fn vsync_is_applied_immediately_rather_than_at_restart() {
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        let display = DisplayStatus::default();
        activate_settings_item(
            SettingsPage::Graphics,
            2,
            &mut ui,
            &mut settings,
            &display,
            1,
        );
        assert!(!settings.vsync_enabled());
        let message = ui.status_message.clone().expect("a status message");
        assert!(
            !message.contains("restart"),
            "VSync applies live now: {message}"
        );
    }
}

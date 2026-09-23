use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::font::{get_char_uv, get_white_uv};
use crate::game::AppState;
use crate::render::Vertex;
use crate::settings::{KeyBindings, Settings};

/// Menu selection state across screens.
#[derive(Debug, Clone, Default)]
pub struct UiState {
    pub main_menu_idx: usize,
    pub level_select_idx: usize,
    pub pause_menu_idx: usize,
    pub settings_idx: usize,
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
    version: &str,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    (app_state as u8).hash(&mut hasher);
    ui_state.main_menu_idx.hash(&mut hasher);
    ui_state.level_select_idx.hash(&mut hasher);
    ui_state.pause_menu_idx.hash(&mut hasher);
    ui_state.settings_idx.hash(&mut hasher);
    ui_state.rebinding_action.hash(&mut hasher);
    ui_state.status_message.as_deref().hash(&mut hasher);
    ui_state.status_is_error.hash(&mut hasher);
    ui_state.level_entries.hash(&mut hasher);
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
    settings.vsync.hash(&mut hasher);
    settings.texture_filtering.hash(&mut hasher);
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
        version: &str,
    ) -> &[Vertex] {
        let signature = ui_signature(app_state, ui_state, settings, version);
        if !self.initialized || signature != self.signature {
            self.vertices = build_ui_geometry(app_state, ui_state, settings, version);
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
            settings_geometry(&mut vertices, ui_state, settings);
        }
    }

    vertices
}

/// Y offset of menu row `index` in the 480x272 reference space.
///
/// Menus hold at most [`SETTINGS_ITEM_COUNT`] rows, so the `u16` conversion is
/// exact and the row offset is lossless.
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

/// Settings screen: panel, rebinding/status prompt, item list and legend.
fn settings_geometry(vertices: &mut Vec<Vertex>, ui_state: &UiState, settings: &Settings) {
    add_rect(vertices, 10.0, 10.0, 470.0, 262.0, [0.08, 0.08, 0.07]);
    add_rect(vertices, 12.0, 12.0, 468.0, 260.0, [0.12, 0.11, 0.10]);

    draw_text(vertices, "SETTINGS", 25.0, 20.0, 2.0, [0.92, 0.88, 0.45]);

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

    settings_item_rows(vertices, ui_state, settings);

    draw_legend(
        vertices,
        "W/S: Move   ENTER/A/D: Adjust   ESC: Back",
        22.0,
        458.0,
        250.0,
    );
}

/// One row per settings item, showing the current binding or value.
fn settings_item_rows(vertices: &mut Vec<Vertex>, ui_state: &UiState, settings: &Settings) {
    let b = &settings.bindings;
    let items = [
        format!("Forward:        [{}]", b.forward),
        format!("Strafe Left:    [{}]", b.strafe_left),
        format!("Strafe Right:   [{}]", b.strafe_right),
        format!("Backward:       [{}]", b.backward),
        format!("Look Up:        [{}]", b.look_up),
        format!("Look Down:      [{}]", b.look_down),
        format!("Look Left:      [{}]", b.look_left),
        format!("Look Right:     [{}]", b.look_right),
        format!("Look Speed H:   [{:.0} deg/s]", settings.look_speed_h),
        format!("Look Speed V:   [{:.0} deg/s]", settings.look_speed_v),
        format!("Walk Speed:     [{:.1} m/s]", settings.walk_speed),
        format!("FOV:            [{:.0} deg]", settings.fov_degrees),
        format!(
            "VSync:          [{}]",
            if settings.vsync { "ON" } else { "OFF" }
        ),
        format!(
            "Filtering:      [{}]",
            settings.texture_filtering.to_uppercase()
        ),
        "Restore Defaults".to_string(),
        "Back".to_string(),
    ];

    let start_y = 44.0;
    let line_h = 13.0;

    for (i, label) in items.iter().enumerate() {
        let y = row_y(i, line_h, start_y);
        let is_sel = i == ui_state.settings_idx;

        if is_sel {
            add_rect(vertices, 23.0, y - 1.0, 450.0, y + 10.0, [0.25, 0.23, 0.16]);
            let line = format!("> {label}");
            draw_text(vertices, &line, 25.0, y, 1.0, [1.0, 0.95, 0.40]);
        } else {
            let line = format!("  {label}");
            draw_text(vertices, &line, 25.0, y, 1.0, [0.85, 0.85, 0.80]);
        }
    }
}

pub const SETTINGS_ITEM_COUNT: usize = 16;

/// Cycles or rebinds selected settings item.
///
/// Returns `true` when the item asks to leave the screen (the Back row).
/// Persisting the change is the caller's job, so this stays a pure UI mutation
/// and a screen can report a failed save itself.
pub fn activate_settings_item(
    idx: usize,
    ui_state: &mut UiState,
    settings: &mut Settings,
    direction: i32,
) -> bool {
    // 0..8: Keybindings
    if let Some(action) = KeyBindings::ACTIONS.get(idx) {
        ui_state.rebinding_action = Some(*action);
        ui_state.clear_status();
        return false;
    }

    match idx {
        8 => {
            // Look Speed H
            let step = if direction < 0 { -15.0 } else { 15.0 };
            settings.look_speed_h += step;
            if settings.look_speed_h > 180.0 {
                settings.look_speed_h = 45.0;
            } else if settings.look_speed_h < 45.0 {
                settings.look_speed_h = 180.0;
            }
            ui_state.set_status("Horizontal look speed updated", false);
        }
        9 => {
            // Look Speed V
            let step = if direction < 0 { -15.0 } else { 15.0 };
            settings.look_speed_v += step;
            if settings.look_speed_v > 150.0 {
                settings.look_speed_v = 30.0;
            } else if settings.look_speed_v < 30.0 {
                settings.look_speed_v = 150.0;
            }
            ui_state.set_status("Vertical look speed updated", false);
        }
        10 => {
            // Walk speed
            let step = if direction < 0 { -0.5 } else { 0.5 };
            settings.walk_speed += step;
            if settings.walk_speed > 6.0 {
                settings.walk_speed = 1.5;
            } else if settings.walk_speed < 1.5 {
                settings.walk_speed = 6.0;
            }
            ui_state.set_status("Walk speed updated", false);
        }
        11 => {
            // FOV
            let step = if direction < 0 { -15.0 } else { 15.0 };
            settings.fov_degrees += step;
            if settings.fov_degrees > 90.0 {
                settings.fov_degrees = 45.0;
            } else if settings.fov_degrees < 45.0 {
                settings.fov_degrees = 90.0;
            }
            ui_state.set_status("Field of view updated", false);
        }
        12 => {
            // VSync applies when the GL context is configured at startup.
            settings.vsync = !settings.vsync;
            ui_state.set_status(
                format!(
                    "VSync {} (applies after restart)",
                    if settings.vsync {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                false,
            );
        }
        13 => {
            // Filtering
            settings.texture_filtering = if settings.texture_filtering == "linear" {
                "nearest".to_string()
            } else {
                "linear".to_string()
            };
            ui_state.set_status(
                format!(
                    "Texture filtering: {}",
                    settings.texture_filtering.to_uppercase()
                ),
                false,
            );
        }
        14 => {
            // Restore defaults: every persisted preference returns to its
            // documented default, including the two without a dedicated row.
            let defaults = Settings::default();
            *settings = defaults;
            ui_state.set_status("Restored default settings", false);
        }
        15 => {
            // Back
            return true; // Signal back
        }
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    // Test code: `expect` documents the invariant being asserted; the
    // production lints stay enforced everywhere else in the crate.
    #![allow(clippy::expect_used)]

    use super::*;
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
    fn every_settings_row_activates_without_breaking_the_settings() {
        // Walk the whole screen in both directions, including the binding rows
        // (which open a rebind), the value rows (which wrap) and the two
        // terminal rows. Back must be the only row that signals leaving.
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        for idx in 0..SETTINGS_ITEM_COUNT {
            for direction in [1, -1] {
                let back = activate_settings_item(idx, &mut ui, &mut settings, direction);
                assert_eq!(
                    back,
                    idx == SETTINGS_ITEM_COUNT - 1,
                    "row {idx} direction {direction} back signal"
                );
                ui.cancel_rebinding();
            }
        }
        settings.sanitize();
        for action in KeyBindings::ACTIONS {
            assert!(settings.bindings.get_key(action).is_some());
        }
    }

    #[test]
    fn restore_defaults_resets_every_persisted_preference() {
        let mut ui = UiState::new();
        let mut settings = Settings {
            look_speed_h: 180.0,
            look_speed_v: 20.0,
            walk_speed: 6.0,
            fov_degrees: 90.0,
            vsync: false,
            texture_filtering: "nearest".to_string(),
            quality: "low".to_string(),
            lightmaps: false,
            ..Settings::default()
        };
        let back = activate_settings_item(14, &mut ui, &mut settings, 1);
        assert!(!back);
        assert_eq!(settings, Settings::default());
        assert_eq!(
            ui.status_message.as_deref(),
            Some("Restored default settings")
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
            let mut ui = UiState::new();
            ui.level_entries = vec![
                "A level name long enough to overflow its panel if it were not shortened"
                    .to_string(),
            ];
            ui.set_status(
                "A status message long enough to run off the right edge if it were not shortened",
                false,
            );
            ui.rebinding_action = Some("strafe_right");
            let vertices = build_ui_geometry(state, &ui, &Settings::default(), "9.9.9");
            assert!(!vertices.is_empty(), "{state:?} drew nothing");
            for vertex in &vertices {
                assert!(
                    (0.0..=480.0).contains(&vertex.pos[0]),
                    "{state:?} vertex x {} is outside the frame",
                    vertex.pos[0]
                );
                assert!(
                    (0.0..=272.0).contains(&vertex.pos[1]),
                    "{state:?} vertex y {} is outside the frame",
                    vertex.pos[1]
                );
            }
        }
    }

    #[test]
    fn vsync_says_it_applies_after_restart() {
        let mut ui = UiState::new();
        let mut settings = Settings::default();
        activate_settings_item(12, &mut ui, &mut settings, 1);
        assert!(!settings.vsync);
        let message = ui.status_message.clone().expect("a status message");
        assert!(message.contains("after restart"), "{message}");
    }
}

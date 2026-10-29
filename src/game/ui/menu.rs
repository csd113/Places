//! Small reference-space layouts shared by rendering and pointer input.
use super::{
    SettingsPage, SettingsRowKind, SettingsValue, UiState, add_rect, add_rect_rgba, draw_text,
    fit_text, row_y, settings_rows, text_width,
};
use crate::{
    engine::render::{DrawableSize, Vertex},
    game::AppState,
    game::display::DisplayStatus,
    game::settings::{Settings, WindowMode},
};
use std::hash::{Hash, Hasher};

const TEXT: [f32; 3] = [1.0, 0.95, 0.84];
const MUTED: [f32; 3] = [0.65, 0.65, 0.59];
const ACCENT: [f32; 3] = [1.0, 0.86, 0.25];
const LEFT: f32 = 36.0;
const RIGHT: f32 = 444.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl MenuRect {
    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0].is_finite()
            && point[1].is_finite()
            && point[0] >= self.left
            && point[0] < self.right
            && point[1] >= self.top
            && point[1] < self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuIntent {
    Activate,
    Adjust(i32),
    Scroll(i32),
    CancelRebind,
    CancelLoad,
    Dismiss,
    RetryLoad,
    Page(i32),
    Volume(u8),
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MenuHit {
    pub index: usize,
    pub intent: MenuIntent,
}

#[derive(Debug)]
pub struct MenuRow {
    pub index: usize,
    pub bounds: MenuRect,
    pub label: String,
    pub value: String,
    pub adjustable: bool,
    pub enabled: bool,
    pub slider: bool,
}

#[derive(Debug, Default)]
pub struct MenuLayout {
    pub rows: Vec<MenuRow>,
    pub scroll: Option<(usize, usize, usize)>,
    pub rebinding: bool,
    pub loading: bool,
    pub overlay: Option<bool>,
    pub load_retry: Option<()>,
    pub details: bool,
    pub details_available: Option<()>,
}

impl MenuLayout {
    /// Tests the very same row rectangles the geometry draws. Arrow controls
    /// have fixed generous bounds; text length never changes their hit region.
    #[must_use]
    pub fn hit_test(&self, point: [f32; 2]) -> Option<MenuHit> {
        if let Some(error) = self.overlay {
            let intent =
                if self.load_retry.is_some() && rect(36.0, 232.0, 260.0, 252.0).contains(point) {
                    MenuIntent::RetryLoad
                } else if error && rect(36.0, 206.0, 444.0, 228.0).contains(point) {
                    MenuIntent::Page(if point[0] < 240.0 { -1 } else { 1 })
                } else {
                    MenuIntent::Dismiss
                };
            return rect(20.0, 12.0, 460.0, 260.0)
                .contains(point)
                .then_some(MenuHit { index: 0, intent });
        }
        if self.loading {
            return rect(LEFT, 172.0, RIGHT, 192.0)
                .contains(point)
                .then_some(MenuHit {
                    index: 0,
                    intent: MenuIntent::CancelLoad,
                });
        }
        if self.rebinding {
            return rect(LEFT, 172.0, RIGHT, 192.0)
                .contains(point)
                .then_some(MenuHit {
                    index: 0,
                    intent: MenuIntent::CancelRebind,
                });
        }
        if self.details {
            return rect(20.0, 12.0, 460.0, 260.0)
                .contains(point)
                .then_some(MenuHit {
                    index: 0,
                    intent: if point[1] >= 232.0 {
                        MenuIntent::Details
                    } else {
                        MenuIntent::Page(if point[1] < 120.0 { -1 } else { 1 })
                    },
                });
        }
        if self.details_available.is_some() && rect(30.0, 238.0, 434.0, 250.0).contains(point) {
            return Some(MenuHit {
                index: 0,
                intent: MenuIntent::Details,
            });
        }
        if let Some((offset, visible, total)) = self.scroll {
            for (top, direction, enabled) in [
                (64.0, -1_i32, offset > 0),
                (186.0, 1_i32, offset.saturating_add(visible) < total),
            ] {
                if enabled && rect(444.0, top, 458.0, top + 16.0).contains(point) {
                    return Some(MenuHit {
                        index: 0,
                        intent: MenuIntent::Scroll(direction),
                    });
                }
            }
        }
        self.rows
            .iter()
            .find(|row| row.bounds.contains(point))
            .map(|row| {
                let intent = if row.slider && point[0] >= 324.0 && point[0] <= 420.0 {
                    let percent = format!(
                        "{:.0}",
                        ((point[0] - 324.0) / 96.0 * 100.0).clamp(0.0, 100.0)
                    )
                    .parse::<u8>()
                    .unwrap_or(0);
                    MenuIntent::Volume(percent)
                } else if row.adjustable && row.enabled && point[0] >= 300.0 && point[0] < 324.0 {
                    MenuIntent::Adjust(-1)
                } else if row.adjustable && row.enabled && point[0] >= 420.0 {
                    MenuIntent::Adjust(1)
                } else {
                    MenuIntent::Activate
                };
                MenuHit {
                    index: row.index,
                    intent,
                }
            })
    }
}

const fn rect(left: f32, top: f32, right: f32, bottom: f32) -> MenuRect {
    MenuRect {
        left,
        top,
        right,
        bottom,
    }
}

pub(super) fn visible_offset(
    scroll: usize,
    selected: usize,
    total: usize,
    visible: usize,
) -> usize {
    let bounded = scroll.min(total.saturating_sub(visible));
    if selected >= total {
        bounded
    } else if selected < bounded {
        selected
    } else if selected >= bounded.saturating_add(visible) {
        selected.saturating_sub(visible.saturating_sub(1))
    } else {
        bounded
    }
}

fn row(index: usize, label: String, value: String, top: f32, height: f32) -> MenuRow {
    MenuRow {
        index,
        label,
        value,
        bounds: rect(LEFT, top, RIGHT, top + height),
        adjustable: false,
        enabled: true,
        slider: false,
    }
}

fn checkpoint_layout(ui: &UiState) -> Option<MenuLayout> {
    let mut layout = MenuLayout::default();
    if let Some((_, available)) = &ui.resume_offer {
        layout.details = false;
        layout.details_available = None;
        for (index, label) in ["Continue checkpoint", "Start new", "Back"]
            .into_iter()
            .enumerate()
        {
            let mut entry = row(
                index,
                label.into(),
                String::new(),
                row_y(index, 27.0, 121.0),
                22.0,
            );
            entry.enabled = index != 0 || *available;
            layout.rows.push(entry);
        }
        return Some(layout);
    }
    None
}

fn base_layout(state: AppState, ui: &UiState) -> MenuLayout {
    MenuLayout {
        rebinding: ui.rebinding_action.is_some(),
        loading: ui.loading_map,
        details: state == AppState::LevelSelect && ui.details_open.is_some(),
        details_available: (state == AppState::LevelSelect
            && ui.level_select_idx < ui.level_entries.len())
        .then_some(()),
        load_retry: ui.load_retry,
        overlay: if ui.status_is_error {
            Some(true)
        } else if ui.credits.is_some() {
            Some(false)
        } else {
            None
        },
        ..MenuLayout::default()
    }
}

/// All selectable rows, including pinned trailing actions and scroll controls.
#[must_use]
pub fn menu_layout(
    state: AppState,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) -> MenuLayout {
    let mut layout = base_layout(state, ui);
    if let Some(resume) = checkpoint_layout(ui) {
        return resume;
    }
    match state {
        AppState::Playing => {}
        AppState::MainMenu => {
            for (index, label) in ["Explore", "Settings", "Credits", "Exit"]
                .into_iter()
                .enumerate()
            {
                let mut entry = row(
                    index,
                    label.to_string(),
                    String::new(),
                    row_y(index, 19.0, 76.0),
                    18.0,
                );
                entry.bounds.left = 24.0;
                entry.bounds.right = 174.0;
                layout.rows.push(entry);
            }
        }
        AppState::Paused => {
            for (index, label) in ["Resume", "Settings", "Return to main menu"]
                .into_iter()
                .enumerate()
            {
                layout.rows.push(row(
                    index,
                    label.to_string(),
                    String::new(),
                    row_y(index, 30.0, 88.0),
                    24.0,
                ));
            }
        }
        AppState::LevelSelect => {
            let total = ui.level_entries.len();
            let offset = visible_offset(ui.level_scroll, ui.level_select_idx, total, 4);
            for (position, (index, label)) in ui
                .level_entries
                .iter()
                .enumerate()
                .skip(offset)
                .take(4)
                .enumerate()
            {
                let col = f32::from(u8::try_from(position % 2).unwrap_or(0));
                let line = f32::from(u8::try_from(position / 2).unwrap_or(0));
                let mut entry = row(
                    index,
                    label.clone(),
                    String::new(),
                    line.mul_add(76.0, 66.0),
                    66.0,
                );
                entry.bounds.left = col.mul_add(208.0, 30.0);
                entry.bounds.right = entry.bounds.left + 196.0;
                layout.rows.push(entry);
            }
            for (position, label) in ["Import levels", "Import folder", "Back"]
                .into_iter()
                .enumerate()
            {
                let mut entry = row(
                    total.saturating_add(position),
                    label.to_string(),
                    String::new(),
                    218.0,
                    18.0,
                );
                entry.bounds.left = row_y(position, 142.0, 30.0);
                entry.bounds.right = entry.bounds.left + 134.0;
                layout.rows.push(entry);
            }
            if total > 4 {
                layout.scroll = Some((offset, 4, total));
            }
        }
        AppState::Settings | AppState::PauseSettings => {
            settings_layout(&mut layout, ui, settings, display);
        }
    }
    layout
}

fn settings_layout(
    layout: &mut MenuLayout,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) {
    let rows = settings_rows(ui.settings_page, settings, display, ui.advanced_expanded);
    let total = rows.len().saturating_sub(1);
    let is_controls = ui.settings_page == SettingsPage::Controls;
    let visible = if is_controls { 7 } else { total };
    let offset = visible_offset(ui.settings_scroll, ui.settings_idx, total, visible);
    let height = if ui.settings_page == SettingsPage::Root {
        24.0
    } else {
        16.0
    };
    let spacing = if ui.settings_page == SettingsPage::Root {
        26.0
    } else {
        17.0
    };
    for (position, (index, setting)) in rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(visible)
        .enumerate()
    {
        let mut entry = row(
            index,
            setting.label.clone(),
            setting.value.clone(),
            row_y(position, spacing, 70.0),
            height,
        );
        entry.adjustable = matches!(setting.kind, SettingsRowKind::Value(_));
        entry.slider = setting.kind == SettingsRowKind::Value(SettingsValue::MasterVolume);
        entry.enabled = match setting.kind {
            SettingsRowKind::Value(SettingsValue::Reflections) => settings.lightmaps_enabled(),
            SettingsRowKind::Value(SettingsValue::Resolution) => {
                display.mode != WindowMode::Fullscreen
            }
            SettingsRowKind::Value(_)
            | SettingsRowKind::Advanced
            | SettingsRowKind::Binding(_)
            | SettingsRowKind::Open(_)
            | SettingsRowKind::RestoreDefaults
            | SettingsRowKind::Back => true,
        };
        layout.rows.push(entry);
    }
    layout
        .rows
        .push(row(total, "Back".to_string(), String::new(), 208.0, 16.0));
    if is_controls {
        layout.scroll = Some((offset, visible, total));
    }
}

const fn selected_index(state: AppState, ui: &UiState) -> usize {
    if ui.resume_offer.is_some() {
        return ui.resume_index;
    }
    match state {
        AppState::MainMenu => ui.main_menu_idx,
        AppState::LevelSelect => ui.level_select_idx,
        AppState::Paused => ui.pause_menu_idx,
        AppState::Settings | AppState::PauseSettings => ui.settings_idx,
        AppState::Playing => 0,
    }
}

fn panel(vertices: &mut Vec<Vertex>, title: &str, subtitle: &str, transparent: bool) {
    let opacity = if transparent { 0.0 } else { 0.94 };
    add_rect_rgba(
        vertices,
        20.0,
        12.0,
        460.0,
        260.0,
        [0.06, 0.06, 0.05, opacity],
    );
    add_rect_rgba(
        vertices,
        22.0,
        14.0,
        458.0,
        258.0,
        [0.12, 0.11, 0.10, opacity],
    );
    draw_text(vertices, title, LEFT, 26.0, 2.0, ACCENT);
    draw_text(
        vertices,
        &fit_text(subtitle, 408.0, 0.875),
        LEFT,
        50.0,
        0.875,
        MUTED,
    );
    add_rect(vertices, LEFT, 62.0, RIGHT, 63.0, [0.25, 0.24, 0.19]);
}

fn small_line(vertices: &mut Vec<Vertex>, text: &str, y: f32, color: [f32; 3]) {
    draw_text(
        vertices,
        &fit_text(text, 408.0, 0.875),
        LEFT,
        y,
        0.875,
        color,
    );
}

/// Menu icon placement shares visible-row scrolling and hit layout.
#[must_use]
pub fn level_icon_bounds(row: &MenuRow) -> [f32; 4] {
    [row.bounds.left + 8.0, row.bounds.top + 10.5, 80.0, 45.0]
}

fn draw_icon_fallback(vertices: &mut Vec<Vertex>, row: &MenuRow) {
    let [left, top, width, height] = level_icon_bounds(row);
    add_rect(
        vertices,
        left,
        top,
        left + width,
        top + height,
        [0.12, 0.14, 0.16],
    );
    draw_text(
        vertices,
        "P",
        width.mul_add(0.5, left) - 4.0,
        height.mul_add(0.5, top) - 4.0,
        1.0,
        MUTED,
    );
}

fn draw_rows(vertices: &mut Vec<Vertex>, layout: &MenuLayout, state: AppState, ui: &UiState) {
    let selected = selected_index(state, ui);
    for entry in &layout.rows {
        let bounds = entry.bounds;
        let active = entry.index == selected;
        if state == AppState::LevelSelect && entry.index < ui.level_entries.len() {
            add_rect_rgba(
                vertices,
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                [0.08, 0.10, 0.11, 0.95],
            );
        }
        if active {
            add_rect_rgba(
                vertices,
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                if state == AppState::MainMenu {
                    [0.035, 0.045, 0.05, 0.45]
                } else {
                    [0.25, 0.23, 0.16, 1.0]
                },
            );
            add_rect(
                vertices,
                bounds.left,
                bounds.top,
                bounds.left + 2.0,
                bounds.bottom,
                ACCENT,
            );
        }
        let color = if !entry.enabled {
            MUTED
        } else if active {
            ACCENT
        } else {
            TEXT
        };
        let has_icon = state == AppState::LevelSelect
            && ui.resume_offer.is_none()
            && entry.index < ui.level_entries.len();
        if has_icon {
            draw_icon_fallback(vertices, entry);
        }
        if has_icon {
            let left = bounds.left + 100.0;
            let width = bounds.right - left - 8.0;
            let lines = wrap(&entry.label, 14);
            for (line, text) in lines.iter().take(3).enumerate() {
                let fitted_title = if line == 2 && lines.len() > 3 {
                    fit_text(&lines.get(2..).unwrap_or_default().join(" "), width, 0.75)
                } else {
                    fit_text(text, width, 0.75)
                };
                draw_text(
                    vertices,
                    &fitted_title,
                    left,
                    row_y(line, 10.0, bounds.top + 18.0),
                    0.75,
                    color,
                );
            }
        } else {
            let main = state == AppState::MainMenu;
            let width = if main {
                140.0
            } else if state == AppState::LevelSelect && ui.resume_offer.is_none() {
                126.0
            } else if entry.value.is_empty() {
                360.0
            } else {
                236.0
            };
            let scale = if main { 1.4 } else { 1.0 };
            draw_text(
                vertices,
                &fit_text(&entry.label, width, scale),
                bounds.left + 8.0,
                bounds.top + 5.0,
                scale,
                color,
            );
        }
        draw_value(vertices, entry, ui, color);
    }
    draw_scroll(vertices, layout, ui);
}
fn draw_scroll(vertices: &mut Vec<Vertex>, layout: &MenuLayout, ui: &UiState) {
    if let Some((offset, visible, total)) = layout.scroll {
        let first = offset.saturating_add(1);
        let last = offset.saturating_add(visible).min(total);
        // Count lives on the right of the subtitle, while its title stays short.
        let count = format!("{first}-{last} / {total}");
        draw_text(
            vertices,
            &count,
            RIGHT - text_width(&count, 0.875),
            50.0,
            0.875,
            MUTED,
        );
        for (top, direction, enabled, symbol) in [
            (64.0, -1_i32, offset > 0, "^"),
            (186.0, 1_i32, last < total, "v"),
        ] {
            let hover = ui.pointer.hover
                == Some(MenuHit {
                    index: 0,
                    intent: MenuIntent::Scroll(direction),
                });
            add_rect(
                vertices,
                444.0,
                top,
                458.0,
                top + 16.0,
                if hover && enabled {
                    [0.36, 0.32, 0.19]
                } else {
                    [0.18, 0.17, 0.13]
                },
            );
            draw_text(
                vertices,
                symbol,
                447.0,
                top + 4.0,
                1.0,
                if enabled { ACCENT } else { [0.37, 0.36, 0.31] },
            );
        }
    }
}

fn draw_value(vertices: &mut Vec<Vertex>, entry: &MenuRow, ui: &UiState, color: [f32; 3]) {
    let bounds = entry.bounds;
    if entry.slider {
        let amount = entry
            .value
            .trim_start_matches("< ")
            .split('%')
            .next()
            .and_then(|text| text.parse::<f32>().ok())
            .unwrap_or(0.0)
            / 100.0;
        let y = bounds.top + 10.0;
        add_rect(vertices, 324.0, y, 420.0, y + 2.0, [0.3, 0.3, 0.27]);
        add_rect(
            vertices,
            324.0,
            y,
            amount.mul_add(96.0, 324.0),
            y + 2.0,
            ACCENT,
        );
        let x = amount.mul_add(96.0, 324.0);
        add_rect(vertices, x - 2.0, y - 3.0, x + 2.0, y + 5.0, TEXT);
        draw_text(
            vertices,
            &format!("{:.0}%", amount * 100.0),
            278.0,
            bounds.top + 5.0,
            0.75,
            color,
        );
        draw_text(vertices, "<", 308.0, bounds.top + 5.0, 1.0, color);
        draw_text(vertices, ">", 428.0, bounds.top + 5.0, 1.0, color);
        return;
    }
    if !entry.value.is_empty() {
        let clean = entry
            .value
            .strip_prefix("< ")
            .and_then(|value| value.split_once(" >"))
            .map_or_else(
                || entry.value.clone(),
                |(value, marker)| format!("{value}{marker}"),
            );
        if entry.adjustable && entry.enabled {
            let value = fit_text(&clean, 96.0, 0.875);
            draw_text(
                vertices,
                &value,
                (96.0 - text_width(&value, 0.875)).mul_add(0.5, 324.0),
                bounds.top + 5.0,
                0.875,
                color,
            );
            for (x, intent, symbol) in [
                (308.0, MenuIntent::Adjust(-1), "<"),
                (428.0, MenuIntent::Adjust(1), ">"),
            ] {
                if ui.pointer.hover
                    == Some(MenuHit {
                        index: entry.index,
                        intent,
                    })
                {
                    add_rect(
                        vertices,
                        x - 8.0,
                        bounds.top,
                        x + 16.0,
                        bounds.bottom,
                        [0.36, 0.32, 0.19],
                    );
                }
                draw_text(vertices, symbol, x, bounds.top + 5.0, 1.0, color);
            }
        } else {
            let value = fit_text(&clean, 140.0, 0.875);
            draw_text(
                vertices,
                &value,
                RIGHT - text_width(&value, 0.875) - 8.0,
                bounds.top + 5.0,
                0.875,
                color,
            );
        }
    }
}

fn description(
    state: AppState,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) -> &'static str {
    match state {
        AppState::MainMenu => match ui.main_menu_idx {
            0 => "Choose a place to explore at your own pace.",
            1 => "Graphics, display, audio and controls.",
            _ => "Close Places.",
        },
        AppState::Paused => match ui.pause_menu_idx {
            0 => "Continue from where you paused.",
            1 => "Adjust preferences while your place stays paused.",
            _ => "Leave this visit and choose another place.",
        },
        AppState::LevelSelect | AppState::Playing => "",
        AppState::Settings | AppState::PauseSettings => {
            let rows = settings_rows(ui.settings_page, settings, display, ui.advanced_expanded);
            match rows.get(ui.settings_idx).map(|row| &row.kind) {
                Some(SettingsRowKind::Open(SettingsPage::Graphics)) => {
                    "Image quality, lighting and effects."
                }
                Some(SettingsRowKind::Open(SettingsPage::Display)) => {
                    "Window mode and window size."
                }
                Some(SettingsRowKind::Open(SettingsPage::Audio)) => {
                    "Volume levels for effects and ambience."
                }
                Some(SettingsRowKind::Open(SettingsPage::Controls)) => {
                    "Key bindings, mouse look and movement."
                }
                Some(SettingsRowKind::RestoreDefaults) => {
                    "Restore all preferences, including audio and controls."
                }
                Some(SettingsRowKind::Binding(_)) => {
                    "Select a binding, then press a key. Esc cancels."
                }
                Some(SettingsRowKind::Advanced) => "Texture filtering and reflections.",
                Some(SettingsRowKind::Value(value)) => match value {
                    SettingsValue::VisualStyle => {
                        "Standard, CRT, Legacy or VHS. Applies immediately; quality stays independent."
                    }
                    SettingsValue::GraphicsQuality => {
                        "Texture detail. Lighting preferences stay independent."
                    }
                    SettingsValue::Blood => {
                        "Dynamic blood marks and droplets. Turning off clears all existing effects."
                    }
                    SettingsValue::Bloom => "A soft glow around bright surfaces.",
                    SettingsValue::Vsync => "Match frame presentation to your display.",
                    SettingsValue::Filtering => {
                        "Clarity of textures viewed at a distance or angle."
                    }
                    SettingsValue::Reflections if !settings.lightmaps_enabled() => {
                        "Select Full lighting to adjust reflections."
                    }
                    SettingsValue::Lightmaps => {
                        "Static or Full lighting; keeps your texture preferences."
                    }
                    SettingsValue::Reflections => "Reflected surroundings on shiny surfaces.",
                    SettingsValue::WindowMode => "Windowed or borderless fullscreen.",
                    SettingsValue::Resolution if display.mode == WindowMode::Fullscreen => {
                        "Fullscreen uses your display's size."
                    }
                    SettingsValue::Resolution => "Window size. The game adapts to display scaling.",
                    SettingsValue::SpatialAudio => {
                        "Choose stereo speakers or measured headphone HRTF."
                    }
                    SettingsValue::MasterVolume => {
                        "Overall sound level. Mute keeps your volume choices."
                    }
                    SettingsValue::EffectsVolume => "Movement and interaction sounds.",
                    SettingsValue::AmbientVolume => "Environmental sounds around you.",
                    SettingsValue::Mute => "Silence all sound; keep your volume choices.",
                    SettingsValue::MouseSensitivity => {
                        "How far the camera turns with mouse movement."
                    }
                    SettingsValue::LookSpeedH | SettingsValue::LookSpeedV => {
                        "Camera turning speed for the look keys."
                    }
                    SettingsValue::InvertLook => "Reverse vertical mouse and key look.",
                    SettingsValue::DeveloperConsole => {
                        "Enable testing commands. Press backtick during play."
                    }
                    SettingsValue::WalkSpeed => "Your movement speed while walking.",
                    SettingsValue::Fov => "How much of your surroundings you can see.",
                },
                Some(SettingsRowKind::Back | SettingsRowKind::Open(SettingsPage::Root)) | None => {
                    ""
                }
            }
        }
    }
}

fn checkpoint_geometry(
    state: AppState,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    if let Some((message, _)) = &ui.resume_offer {
        panel(
            &mut vertices,
            "CONTINUE YOUR VISIT",
            "Start new keeps the old save until another checkpoint.",
            false,
        );
        for (index, line) in wrap(message, 60).iter().take(4).enumerate() {
            draw_text(
                &mut vertices,
                line,
                36.0,
                row_y(index, 11.0, 64.0),
                0.875,
                MUTED,
            );
        }
        draw_rows(
            &mut vertices,
            &menu_layout(state, ui, settings, display),
            state,
            ui,
        );
        return vertices;
    }
    vertices
}

fn screen_titles(state: AppState, ui: &UiState) -> (&str, &str) {
    let (title, subtitle) = match state {
        AppState::MainMenu => ("Places", "an experience"),
        AppState::Paused => (
            "PAUSED",
            ui.current_map.as_deref().unwrap_or("Your current visit"),
        ),
        AppState::LevelSelect => ("EXPLORE", "Choose a place. Tab: details."),
        AppState::Settings | AppState::PauseSettings => (
            ui.settings_page.title(),
            match ui.settings_page {
                SettingsPage::Root => "Preferences are saved as you change them.",
                SettingsPage::Graphics => "Settings / Graphics",
                SettingsPage::Display => "Settings / Display",
                SettingsPage::Audio => "Settings / Audio",
                SettingsPage::Controls => {
                    if ui.settings_idx < 11 {
                        "Key bindings"
                    } else {
                        "Look and movement"
                    }
                }
            },
        ),
        AppState::Playing => ("", ""),
    };
    (title, subtitle)
}

pub(super) fn geometry(
    state: AppState,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
    version: &str,
    drawable: DrawableSize,
) -> Vec<Vertex> {
    if ui.loading_map {
        return loading(ui, version);
    }
    if state == AppState::Playing {
        return Vec::new();
    }
    let mut vertices = Vec::new();
    if state == AppState::MainMenu {
        return main_geometry(ui, settings, version, drawable);
    }
    if ui.resume_offer.is_some() {
        return checkpoint_geometry(state, ui, settings, display);
    }
    let (title, subtitle) = screen_titles(state, ui);
    panel(&mut vertices, title, subtitle, state == AppState::MainMenu);
    let layout = menu_layout(state, ui, settings, display);
    draw_rows(&mut vertices, &layout, state, ui);
    draw_empty_state(&mut vertices, state, ui);
    if state == AppState::LevelSelect && ui.level_select_idx < ui.level_entries.len() {
        draw_text(
            &mut vertices,
            "Tab / click: about selected place",
            30.0,
            240.0,
            0.75,
            MUTED,
        );
    }
    if layout.details {
        draw_description(&mut vertices, ui);
        return vertices;
    }
    let help_y = if state == AppState::LevelSelect {
        0.0
    } else {
        226.0
    };
    if help_y > 0.0 && !ui.status_is_error {
        small_line(
            &mut vertices,
            description(state, ui, settings, display),
            help_y,
            MUTED,
        );
    }
    if let Some(message) = &ui.status_message {
        if ui.status_is_error {
            draw_error(&mut vertices, ui);
        } else {
            draw_status(&mut vertices, message, false);
        }
    } else if state == AppState::MainMenu {
        small_line(&mut vertices, &format!("v{version}"), 238.0, MUTED);
    } else if ui.settings_page == SettingsPage::Graphics
        && matches!(state, AppState::Settings | AppState::PauseSettings)
        && super::any_graphics_override(settings)
    {
        small_line(&mut vertices, "* Startup override", 238.0, MUTED);
    }
    let legend = if state == AppState::MainMenu {
        "Click / Enter: select   W/S or arrows: move"
    } else if state == AppState::Paused {
        "Click / Enter: select   Esc: resume"
    } else if matches!(state, AppState::Settings | AppState::PauseSettings)
        && ui.settings_page != SettingsPage::Root
    {
        "Click: select/change   Arrows: move/change   Esc: back"
    } else {
        "Click / Enter: select   W/S or arrows: move   Esc: back"
    };
    if !ui.loading_map && ui.rebinding_action.is_none() {
        small_line(&mut vertices, legend, 250.0, MUTED);
    }
    draw_modals(&mut vertices, ui);
    if ui.status_is_error {
        draw_error(&mut vertices, ui);
    }

    vertices
}

fn draw_empty_state(vertices: &mut Vec<Vertex>, state: AppState, ui: &UiState) {
    if state == AppState::LevelSelect && ui.level_entries.is_empty() {
        small_line(vertices, "No maps installed.", 108.0, MUTED);
        small_line(
            vertices,
            "Copy compiled .placesmap files to Import.",
            124.0,
            MUTED,
        );
        small_line(
            vertices,
            "Then choose Import levels to install them.",
            136.0,
            MUTED,
        );
        small_line(
            vertices,
            &fit_text(&format!("Import: {}", ui.import_directory), 432.0, 0.7),
            152.0,
            MUTED,
        );
    } else if state == AppState::MainMenu && ui.level_entries.is_empty() {
        small_line(
            vertices,
            "No maps installed. Open Explore to add maps.",
            198.0,
            MUTED,
        );
    }
}

/// Errors get two bounded lines above the navigation legend. Full diagnostics
/// remain in the log; external names cannot push text off the panel.
fn draw_status(vertices: &mut Vec<Vertex>, message: &str, is_error: bool) {
    if is_error {
        let split = message
            .chars()
            .take(58)
            .enumerate()
            .filter_map(|(index, character)| {
                (index >= 30 && character.is_whitespace()).then_some(index.saturating_add(1))
            })
            .last()
            .unwrap_or(58);
        let first: String = message.chars().take(split).collect();
        let rest: String = message.chars().skip(split).take(59).collect();
        small_line(vertices, first.trim_end(), 234.0, [1.0, 0.57, 0.43]);
        small_line(vertices, &rest, 242.0, [1.0, 0.57, 0.43]);
    } else {
        small_line(vertices, message, 238.0, [0.68, 0.82, 0.63]);
    }
}

fn draw_modals(vertices: &mut Vec<Vertex>, ui: &UiState) {
    if let Some(action) = ui.rebinding_action {
        add_rect_rgba(vertices, 22.0, 14.0, 458.0, 258.0, [0.06, 0.06, 0.05, 0.94]);
        draw_text(vertices, "PRESS A KEY", LEFT, 100.0, 2.0, ACCENT);
        small_line(
            vertices,
            &format!("Binding: {}", crate::game::settings::action_label(action)),
            132.0,
            TEXT,
        );
        add_rect(vertices, LEFT, 172.0, RIGHT, 192.0, [0.25, 0.23, 0.16]);
        draw_text(vertices, "Cancel", 48.0, 178.0, 1.0, ACCENT);
        small_line(
            vertices,
            "Esc or click Cancel to keep the current binding.",
            208.0,
            MUTED,
        );
    }
}

pub(super) fn loading(ui: &UiState, _version: &str) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    add_rect(
        &mut vertices,
        -480.0,
        -272.0,
        960.0,
        544.0,
        [0.018, 0.026, 0.032],
    );
    let label = "Loading level...";
    draw_text(
        &mut vertices,
        label,
        (480.0 - text_width(label, 1.0)) * 0.5,
        116.0,
        1.0,
        TEXT,
    );
    add_rect(
        &mut vertices,
        120.0,
        140.0,
        360.0,
        144.0,
        [0.16, 0.19, 0.20],
    );
    // Stage completion, not an estimate of elapsed time. Decoding and the final
    // GPU fence have unknown duration; they never acquire invented percentages.
    let right = (f32::from(ui.loading_step.min(7)) / 7.0).mul_add(240.0, 120.0);
    if right > 120.0 {
        add_rect(
            &mut vertices,
            120.0,
            140.0,
            right,
            144.0,
            [0.83, 0.78, 0.55],
        );
    }
    if ui.loading_step < 7 {
        // Travelling activity has no percentage meaning. The solid completed
        // portion above changes only when a real preparation milestone finishes.
        let travel = (348.0 - right).max(0.0);
        let marker = (f32::from(ui.loading_activity) / 72.0).mul_add(travel, right);
        add_rect(
            &mut vertices,
            marker,
            140.0,
            (marker + 12.0).min(360.0),
            144.0,
            [0.48, 0.54, 0.55],
        );
    }
    if ui.loading_map {
        let hover = ui
            .pointer
            .hover
            .is_some_and(|hit| hit.intent == MenuIntent::CancelLoad);
        if hover {
            add_rect_rgba(
                &mut vertices,
                LEFT,
                172.0,
                RIGHT,
                192.0,
                [0.12, 0.15, 0.16, 1.0],
            );
        }
        let cancel = "Esc / click: cancel";
        draw_text(
            &mut vertices,
            cancel,
            (480.0 - text_width(cancel, 0.75)) * 0.5,
            179.0,
            0.75,
            MUTED,
        );
    }
    vertices
}

impl UiState {
    /// Vertical grid movement never skips a pinned action on an odd last row.
    pub(crate) fn move_level_row(&mut self, up: bool) {
        let total = self.level_entries.len();
        let selected = self.level_select_idx;
        self.level_select_idx = if total == 0 {
            if up {
                selected.checked_sub(1).unwrap_or(2)
            } else if selected < 2 {
                selected.saturating_add(1)
            } else {
                0
            }
        } else if selected >= total {
            if up { total.saturating_sub(1) } else { 0 }
        } else if up {
            if selected >= 2 {
                selected.saturating_sub(2)
            } else {
                total.saturating_add(2)
            }
        } else {
            selected.saturating_add(2).min(total)
        };
        self.keep_selection_visible(AppState::LevelSelect);
    }

    pub(crate) fn place_details(&self) -> String {
        let name = self
            .level_entries
            .get(self.level_select_idx)
            .map_or("", String::as_str);
        if self.level_description.lines().next() == Some(name) {
            self.level_description.clone()
        } else {
            format!("{name}\n\n{}", self.level_description)
        }
    }
    pub const fn select_menu_index(&mut self, state: AppState, index: usize) {
        if self.resume_offer.is_some() {
            self.resume_index = if index > 2 { 2 } else { index };
            return;
        }
        match state {
            AppState::MainMenu => self.main_menu_idx = index,
            AppState::LevelSelect => self.level_select_idx = index,
            AppState::Paused => self.pause_menu_idx = index,
            AppState::Settings | AppState::PauseSettings => self.settings_idx = index,
            AppState::Playing => {}
        }
    }

    pub fn keep_selection_visible(&mut self, state: AppState) {
        if state == AppState::LevelSelect {
            self.level_scroll = visible_offset(
                self.level_scroll,
                self.level_select_idx,
                self.level_entries.len(),
                4,
            );
        } else if matches!(state, AppState::Settings | AppState::PauseSettings)
            && self.settings_page == SettingsPage::Controls
        {
            self.settings_scroll = visible_offset(
                self.settings_scroll,
                self.settings_idx,
                self.settings_page.item_count(false).saturating_sub(1),
                7,
            );
        }
    }

    pub fn scroll_details(&mut self, direction: i32) {
        if self.status_is_error {
            let maximum = wrap(self.status_message.as_deref().unwrap_or(""), 56)
                .len()
                .div_ceil(12)
                .saturating_sub(1);
            self.status_page = if direction < 0_i32 {
                self.status_page.saturating_sub(1)
            } else {
                self.status_page.saturating_add(1).min(maximum)
            };
        } else {
            let maximum = super::description_pages(&self.place_details()).saturating_sub(1);
            self.description_page = if direction < 0_i32 {
                self.description_page.saturating_sub(1)
            } else {
                self.description_page.saturating_add(1).min(maximum)
            };
        }
    }

    pub fn scroll_menu(&mut self, state: AppState, direction: i32) {
        if self.status_is_error {
            self.scroll_details(direction);
            return;
        }
        self.keep_selection_visible(state);
        let (scroll, selection, total, visible) = if state == AppState::LevelSelect {
            (
                &mut self.level_scroll,
                &mut self.level_select_idx,
                self.level_entries.len(),
                4,
            )
        } else if matches!(state, AppState::Settings | AppState::PauseSettings)
            && self.settings_page == SettingsPage::Controls
        {
            (
                &mut self.settings_scroll,
                &mut self.settings_idx,
                SettingsPage::Controls.item_count(false).saturating_sub(1),
                7,
            )
        } else {
            return;
        };
        *scroll = if direction < 0_i32 {
            scroll.saturating_sub(1)
        } else {
            scroll.saturating_add(1).min(total.saturating_sub(visible))
        };
        if *selection < total {
            *selection = (*selection).clamp(
                *scroll,
                scroll
                    .saturating_add(visible.saturating_sub(1))
                    .min(total.saturating_sub(1)),
            );
        }
        self.pointer.invalidate();
    }

    /// Selection changes do not invalidate hover. Structural changes do,
    /// including the exact logical/drawable size pair after a monitor move.
    pub fn sync_pointer_context(
        &mut self,
        state: AppState,
        window: (u32, u32),
        drawable: (u32, u32),
        interactive: bool,
    ) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (
            super::app_state_tag(state),
            self.settings_page,
            self.advanced_expanded,
            self.rebinding_action,
            self.level_entries.len(),
            self.level_scroll,
            self.settings_scroll,
            (self.loading_map, self.details_open),
            (self.credits, self.status_is_error),
            window,
            drawable,
            interactive,
        )
            .hash(&mut hasher);
        self.pointer.sync_context(hasher.finish());
    }
}

/// Character-boundary wrapping with preserved paragraph breaks.
pub(super) fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty()
                && line
                    .chars()
                    .count()
                    .saturating_add(word.chars().count())
                    .saturating_add(1)
                    > width
            {
                lines.push(line.trim_end().to_string());
                line.clear();
            }
            for character in word.chars() {
                if line.chars().count() >= width {
                    lines.push(line.trim_end().to_string());
                    line.clear();
                }
                line.push(character);
            }
            line.push(' ');
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}
fn draw_description(vertices: &mut Vec<Vertex>, ui: &UiState) {
    add_rect_rgba(
        vertices,
        20.0,
        12.0,
        460.0,
        260.0,
        [0.025, 0.035, 0.04, 1.0],
    );
    draw_text(vertices, "ABOUT THIS PLACE", 36.0, 30.0, 1.25, ACCENT);
    let lines = wrap(&ui.place_details(), 56);
    let pages = lines.len().div_ceil(12).max(1);
    let page = ui.description_page.min(pages.saturating_sub(1));
    for (line, text) in lines
        .iter()
        .skip(page.saturating_mul(12))
        .take(12)
        .enumerate()
    {
        draw_text(vertices, text, 36.0, row_y(line, 11.0, 63.0), 0.875, TEXT);
    }
    draw_text(
        vertices,
        &format!(
            "Details {}/{}   Arrows / wheel: scroll",
            page.saturating_add(1),
            pages
        ),
        36.0,
        212.0,
        0.875,
        MUTED,
    );
    add_rect(vertices, 36.0, 232.0, 444.0, 252.0, [0.22, 0.21, 0.15]);
    draw_text(
        vertices,
        "Tab / Enter / Esc / click: back",
        46.0,
        238.0,
        0.875,
        ACCENT,
    );
}

fn draw_error(vertices: &mut Vec<Vertex>, ui: &UiState) {
    add_rect_rgba(
        vertices,
        20.0,
        12.0,
        460.0,
        260.0,
        [0.025, 0.035, 0.04, 0.99],
    );
    draw_text(
        vertices,
        if ui.load_retry.is_some() {
            "MAP NEEDS A FRESH START"
        } else {
            "SOMETHING WENT WRONG"
        },
        36.0,
        30.0,
        1.5,
        [1.0, 0.63, 0.45],
    );
    let lines = wrap(
        ui.status_message.as_deref().unwrap_or("Please try again."),
        56,
    );
    let pages = lines.len().div_ceil(12).max(1);
    let page = ui.status_page.min(pages.saturating_sub(1));
    for (line, text) in lines
        .iter()
        .skip(page.saturating_mul(12))
        .take(12)
        .enumerate()
    {
        draw_text(vertices, text, 36.0, row_y(line, 11.0, 63.0), 0.875, TEXT);
    }
    draw_text(
        vertices,
        &format!(
            "Details {}/{}   Arrows / wheel: scroll",
            page.saturating_add(1),
            pages
        ),
        36.0,
        212.0,
        0.875,
        MUTED,
    );
    if ui.load_retry.is_some() {
        add_rect(vertices, 36.0, 232.0, 260.0, 252.0, [0.22, 0.21, 0.15]);
        add_rect(vertices, 274.0, 232.0, 444.0, 252.0, [0.13, 0.18, 0.19]);
        draw_text(
            vertices,
            "Enter: unload + retry",
            46.0,
            238.0,
            0.875,
            ACCENT,
        );
        draw_text(vertices, "Esc: keep map", 286.0, 238.0, 0.875, TEXT);
        return;
    }
    add_rect(vertices, 36.0, 232.0, 444.0, 252.0, [0.22, 0.21, 0.15]);
    draw_text(
        vertices,
        "Enter / Esc: dismiss and try again",
        46.0,
        238.0,
        0.875,
        ACCENT,
    );
}
fn main_geometry(
    ui: &UiState,
    settings: &Settings,
    version: &str,
    drawable: DrawableSize,
) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    let [left, top, _, bottom] = drawable.ui_bounds();
    // Keep the existing fade at x=0..224; fill the aspect-fit margin at its
    // left-edge opacity so the scene cannot show through at full brightness.
    if left < 0.0 {
        add_rect_rgba(
            &mut vertices,
            left,
            top,
            0.0,
            bottom,
            [0.018, 0.026, 0.032, 0.74],
        );
    }
    let fade_start = vertices.len();
    // Interpolate alpha across one quad. Separate constant-alpha strips made
    // visible vertical bands over the otherwise continuous scene lighting.
    add_rect_rgba(
        &mut vertices,
        0.0,
        top,
        224.0,
        bottom,
        [0.018, 0.026, 0.032, 0.74],
    );
    for vertex in vertices.iter_mut().skip(fade_start) {
        vertex.color[3] = (1.0 - vertex.pos[0] / 224.0) * 0.74;
    }
    let logo_start = vertices.len();
    draw_text(&mut vertices, "Places", 23.0, 22.0, 5.0, TEXT);
    // Keep the reference's tall, compact wordmark proportions with the committed atlas.
    for vertex in vertices.iter_mut().skip(logo_start) {
        if let Some(x) = vertex.pos.first_mut() {
            *x = (*x - 23.0).mul_add(0.60, 23.0);
        }
    }

    let layout = menu_layout(AppState::MainMenu, ui, settings, &DisplayStatus::default());
    draw_rows(&mut vertices, &layout, AppState::MainMenu, ui);
    draw_text(
        &mut vertices,
        "Enter: select   Arrows: move",
        25.0,
        244.0,
        0.75,
        MUTED,
    );
    draw_text(
        &mut vertices,
        &format!("v{version}"),
        25.0,
        257.0,
        0.65,
        MUTED,
    );
    if ui.credits.is_some() {
        add_rect_rgba(
            &mut vertices,
            20.0,
            12.0,
            460.0,
            260.0,
            [0.025, 0.035, 0.04, 0.97],
        );
        draw_text(&mut vertices, "PLACES", 36.0, 32.0, 3.0, TEXT);
        for (line, text) in [
            "Created by Connor Dawkins",
            "",
            "Built with Rust, wgpu, SDL3, Luau and Kira.",
            "Open source licenses accompany this installation.",
            "",
        ]
        .into_iter()
        .enumerate()
        {
            draw_text(
                &mut vertices,
                text,
                36.0,
                row_y(line, 17.0, 84.0),
                0.875,
                TEXT,
            );
        }
        draw_text(
            &mut vertices,
            "Enter / Esc / click: back",
            36.0,
            236.0,
            1.0,
            ACCENT,
        );
    }
    if ui.status_is_error {
        draw_error(&mut vertices, ui);
    }
    draw_modals(&mut vertices, ui);
    vertices
}

#[cfg(test)]
#[path = "../tests/menu.rs"]
mod tests;

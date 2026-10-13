//! Small reference-space layouts shared by rendering and pointer input.
use super::{
    SettingsPage, SettingsRowKind, SettingsValue, UiState, add_rect, add_rect_rgba, draw_text,
    fit_text, row_y, settings_rows, text_width,
};
use crate::{
    display::DisplayStatus,
    game::AppState,
    render::Vertex,
    settings::{Settings, WindowMode},
};
use std::hash::{Hash, Hasher};

const TEXT: [f32; 3] = [0.88, 0.87, 0.80];
const MUTED: [f32; 3] = [0.65, 0.65, 0.59];
const ACCENT: [f32; 3] = [0.96, 0.91, 0.44];
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
}

#[derive(Debug, Default)]
pub struct MenuLayout {
    pub rows: Vec<MenuRow>,
    pub scroll: Option<(usize, usize, usize)>,
    pub rebinding: bool,
    pub loading: bool,
}

impl MenuLayout {
    /// Tests the very same row rectangles the geometry draws. Arrow controls
    /// have fixed generous bounds; text length never changes their hit region.
    #[must_use]
    pub fn hit_test(&self, point: [f32; 2]) -> Option<MenuHit> {
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
                let intent =
                    if row.adjustable && row.enabled && point[0] >= 300.0 && point[0] < 324.0 {
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
    let mut layout = MenuLayout {
        rebinding: ui.rebinding_action.is_some(),
        loading: ui.loading_map,
        ..MenuLayout::default()
    };
    match state {
        AppState::Playing => {}
        AppState::MainMenu | AppState::Paused => {
            let labels = if state == AppState::MainMenu {
                ["Explore maps", "Settings", "Exit"]
            } else {
                ["Resume", "Settings", "Return to main menu"]
            };
            for (index, label) in labels.into_iter().enumerate() {
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
            let offset = visible_offset(ui.level_scroll, ui.level_select_idx, total, 6);
            for (visible, (index, label)) in ui
                .level_entries
                .iter()
                .enumerate()
                .skip(offset)
                .take(6)
                .enumerate()
            {
                layout.rows.push(row(
                    index,
                    label.clone(),
                    String::new(),
                    row_y(visible, 20.0, 70.0),
                    18.0,
                ));
            }
            layout.rows.push(row(
                total,
                "Import levels".to_string(),
                String::new(),
                194.0,
                18.0,
            ));
            layout.rows.push(row(
                total.saturating_add(1),
                "Back".to_string(),
                String::new(),
                214.0,
                18.0,
            ));
            if total > 6 {
                layout.scroll = Some((offset, 6, total));
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
        29.0
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
    match state {
        AppState::MainMenu => ui.main_menu_idx,
        AppState::LevelSelect => ui.level_select_idx,
        AppState::Paused => ui.pause_menu_idx,
        AppState::Settings | AppState::PauseSettings => ui.settings_idx,
        AppState::Playing => 0,
    }
}

fn panel(vertices: &mut Vec<Vertex>, title: &str, subtitle: &str, transparent: bool) {
    let opacity = if transparent { 0.88 } else { 1.0 };
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

fn draw_rows(vertices: &mut Vec<Vertex>, layout: &MenuLayout, state: AppState, ui: &UiState) {
    let selected = selected_index(state, ui);
    for entry in &layout.rows {
        let bounds = entry.bounds;
        let active = entry.index == selected;
        if active {
            add_rect(
                vertices,
                bounds.left,
                bounds.top,
                bounds.right,
                bounds.bottom,
                [0.25, 0.23, 0.16],
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
        let label_width = if entry.value.is_empty() { 360.0 } else { 236.0 };
        draw_text(
            vertices,
            &fit_text(&entry.label, label_width, 1.0),
            48.0,
            bounds.top + 5.0,
            1.0,
            color,
        );
        draw_value(vertices, entry, ui, color);
    }
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
            1 => "Graphics, display and controls.",
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
                Some(SettingsRowKind::Open(SettingsPage::Controls)) => {
                    "Key bindings, mouse look and movement."
                }
                Some(SettingsRowKind::RestoreDefaults) => {
                    "Restore all graphics, display and control preferences."
                }
                Some(SettingsRowKind::Binding(_)) => {
                    "Select a binding, then press a key. Esc cancels."
                }
                Some(SettingsRowKind::Advanced) => "Texture filtering and reflections.",
                Some(SettingsRowKind::Value(value)) => match value {
                    SettingsValue::GraphicsQuality => {
                        "Texture detail. Lighting preferences stay independent."
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

pub(super) fn geometry(
    state: AppState,
    ui: &UiState,
    settings: &Settings,
    display: &DisplayStatus,
    version: &str,
) -> Vec<Vertex> {
    if state == AppState::Playing {
        return Vec::new();
    }
    let mut vertices = Vec::new();
    let (title, subtitle) = match state {
        AppState::MainMenu => ("Places", "an experience"),
        AppState::Paused => (
            "PAUSED",
            ui.current_map.as_deref().unwrap_or("Your current visit"),
        ),
        AppState::LevelSelect => ("EXPLORE", "Installed maps"),
        AppState::Settings | AppState::PauseSettings => (
            ui.settings_page.title(),
            match ui.settings_page {
                SettingsPage::Root => "Preferences are saved as you change them.",
                SettingsPage::Graphics => "Settings / Graphics",
                SettingsPage::Display => "Settings / Display",
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
    panel(&mut vertices, title, subtitle, state == AppState::MainMenu);
    let layout = menu_layout(state, ui, settings, display);
    draw_rows(&mut vertices, &layout, state, ui);
    draw_empty_state(&mut vertices, state, ui);
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
        draw_status(&mut vertices, message, ui.status_is_error);
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

    vertices
}

fn draw_empty_state(vertices: &mut Vec<Vertex>, state: AppState, ui: &UiState) {
    if state == AppState::LevelSelect && ui.level_entries.is_empty() {
        small_line(vertices, "No maps installed.", 108.0, MUTED);
        small_line(
            vertices,
            "Add .placesmap files to levels/ or import/.",
            124.0,
            MUTED,
        );
        small_line(
            vertices,
            "Import levels rescans both folders.",
            136.0,
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
    if ui.loading_map {
        add_rect_rgba(vertices, 22.0, 64.0, 458.0, 258.0, [0.06, 0.06, 0.05, 0.96]);
        draw_text(vertices, "LOADING MAP", LEFT, 100.0, 1.5, ACCENT);
        small_line(
            vertices,
            ui.status_message.as_deref().unwrap_or("Preparing level..."),
            132.0,
            TEXT,
        );
        add_rect(vertices, LEFT, 172.0, RIGHT, 192.0, [0.25, 0.23, 0.16]);
        draw_text(vertices, "Cancel", 48.0, 178.0, 1.0, ACCENT);
        small_line(vertices, "Esc or click Cancel to return.", 208.0, MUTED);
    }
    if let Some(action) = ui.rebinding_action {
        add_rect_rgba(vertices, 22.0, 14.0, 458.0, 258.0, [0.06, 0.06, 0.05, 0.94]);
        draw_text(vertices, "PRESS A KEY", LEFT, 100.0, 2.0, ACCENT);
        small_line(
            vertices,
            &format!("Binding: {}", crate::settings::action_label(action)),
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

pub(super) fn loading(ui: &UiState, version: &str) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    panel(&mut vertices, "Places", "Preparing your place", true);
    small_line(
        &mut vertices,
        ui.status_message.as_deref().unwrap_or("Preparing level..."),
        128.0,
        if ui.status_is_error {
            [1.0, 0.57, 0.43]
        } else {
            TEXT
        },
    );
    small_line(&mut vertices, &format!("v{version}"), 238.0, MUTED);
    vertices
}

impl UiState {
    pub const fn select_menu_index(&mut self, state: AppState, index: usize) {
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
                6,
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

    pub fn scroll_menu(&mut self, state: AppState, direction: i32) {
        self.keep_selection_visible(state);
        let (scroll, selection, total, visible) = if state == AppState::LevelSelect {
            (
                &mut self.level_scroll,
                &mut self.level_select_idx,
                self.level_entries.len(),
                6,
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
            self.loading_map,
            window,
            drawable,
            interactive,
        )
            .hash(&mut hasher);
        self.pointer.sync_context(hasher.finish());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No packages and an arbitrary long error must leave both recovery rows
    /// clickable. Geometry stays within the reference canvas at fixed cost.
    #[test]
    fn empty_explore_keeps_import_and_back_usable_with_bounded_errors() {
        let mut ui = UiState::new();
        ui.set_status("Invalid optional package: ".repeat(100), true);
        let settings = Settings::default();
        let display = DisplayStatus::default();
        let layout = menu_layout(AppState::LevelSelect, &ui, &settings, &display);
        assert_eq!(
            layout.rows.len(),
            2,
            "empty list retains recovery navigation"
        );
        for row in &layout.rows {
            assert_eq!(
                layout
                    .hit_test([row.bounds.left + 8.0, row.bounds.top + 4.0])
                    .map(|hit| hit.index),
                Some(row.index)
            );
        }
        let vertices = geometry(AppState::LevelSelect, &ui, &settings, &display, "test");
        let error_vertices: Vec<_> = vertices
            .iter()
            .filter(|vertex| {
                vertex.color[0].total_cmp(&1.0).is_eq() && vertex.color[1].total_cmp(&0.57).is_eq()
            })
            .collect();
        assert_ne!(error_vertices.len(), 0, "the error remains visible");
        assert!(
            error_vertices
                .iter()
                .all(|vertex| vertex.pos[1] >= 232.0 && vertex.pos[1] <= 249.0),
            "error text must clear Back and the navigation legend"
        );
        assert!(
            vertices
                .iter()
                .all(|vertex| (0.0..=480.0).contains(&vertex.pos[0])
                    && (0.0..=272.0).contains(&vertex.pos[1])),
            "optional package errors remain on screen"
        );
        assert!(
            vertices.len() < 3_000,
            "diagnostic size does not scale with external text"
        );
    }

    #[test]
    fn every_selection_is_visible_and_hit_regions_match_drawn_rows() {
        let settings = Settings::default();
        let display = DisplayStatus::default();
        for state in [
            AppState::MainMenu,
            AppState::Paused,
            AppState::LevelSelect,
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
                ui.advanced_expanded = true;
                ui.level_entries = (0_usize..20).map(|index| format!("Map {index}")).collect();
                let count = match state {
                    AppState::MainMenu | AppState::Paused => 3,
                    AppState::LevelSelect => 22,
                    AppState::Settings | AppState::PauseSettings => page.item_count(true),
                    AppState::Playing => 0,
                };
                for selected in 0..count {
                    ui.select_menu_index(state, selected);
                    ui.keep_selection_visible(state);
                    let layout = menu_layout(state, &ui, &settings, &display);
                    assert!(
                        layout.rows.iter().any(|entry| entry.index == selected),
                        "selection {selected} hidden on {state:?}/{page:?}"
                    );
                    for entry in &layout.rows {
                        let point = [entry.bounds.left + 12.0, entry.bounds.top + 6.0];
                        assert_eq!(
                            layout.hit_test(point),
                            Some(MenuHit {
                                index: entry.index,
                                intent: MenuIntent::Activate
                            }),
                            "drawn row hit mismatch"
                        );
                        assert!(
                            entry.bounds.top >= 64.0 && entry.bounds.bottom <= 232.0,
                            "row leaves list area"
                        );
                        for other in layout
                            .rows
                            .iter()
                            .filter(|other| other.index != entry.index)
                        {
                            assert!(
                                entry.bounds.bottom <= other.bounds.top
                                    || entry.bounds.top >= other.bounds.bottom,
                                "overlapping controls {state:?}/{page:?}"
                            );
                        }
                    }
                    assert_eq!(layout.hit_test([10.0, 10.0]), None, "outside panel");
                    assert_eq!(
                        layout.hit_test([f32::NAN, 80.0]),
                        None,
                        "invalid coordinate"
                    );
                }
            }
        }
    }

    #[test]
    fn arrow_targets_are_consistent_and_disabled_values_explain_without_adjusting() {
        let mut ui = UiState::new();
        ui.settings_page = SettingsPage::Graphics;
        ui.advanced_expanded = true;
        let mut settings = Settings::default();
        for disabled in [false, true] {
            settings.use_low_quality_lighting = disabled;
            let layout = menu_layout(
                AppState::Settings,
                &ui,
                &settings,
                &DisplayStatus::default(),
            );
            for entry in layout.rows.iter().filter(|entry| entry.adjustable) {
                for (x, direction) in [(308.0, -1_i32), (428.0, 1_i32)] {
                    let intent = if entry.enabled {
                        MenuIntent::Adjust(direction)
                    } else {
                        MenuIntent::Activate
                    };
                    assert_eq!(
                        layout.hit_test([x, entry.bounds.top + 6.0]),
                        Some(MenuHit {
                            index: entry.index,
                            intent
                        }),
                        "arrow targets match rendered state"
                    );
                }
            }
        }
    }

    #[test]
    fn hover_never_moves_a_scrolled_list_under_the_pointer_and_back_stays_pinned() {
        let mut ui = UiState::new();
        ui.level_entries = (0_usize..20).map(|index| index.to_string()).collect();
        ui.level_scroll = 7;
        ui.level_select_idx = 7;
        ui.keep_selection_visible(AppState::LevelSelect);
        let before = menu_layout(
            AppState::LevelSelect,
            &ui,
            &Settings::default(),
            &DisplayStatus::default(),
        );
        ui.level_select_idx = 12;
        ui.keep_selection_visible(AppState::LevelSelect);
        let after = menu_layout(
            AppState::LevelSelect,
            &ui,
            &Settings::default(),
            &DisplayStatus::default(),
        );
        assert_eq!(
            before.scroll, after.scroll,
            "hover within view keeps layout stable"
        );
        assert_eq!(
            before.rows.last().map(|entry| entry.bounds),
            after.rows.last().map(|entry| entry.bounds),
            "Back remains pinned"
        );
        ui.scroll_menu(AppState::LevelSelect, 1);
        assert_eq!(ui.level_scroll, 8, "scroll moves content by one row");
    }

    #[test]
    fn modal_cancel_is_the_only_pointer_target() {
        let mut ui = UiState::new();
        ui.rebinding_action = Some("forward");
        let rebind = menu_layout(
            AppState::Settings,
            &ui,
            &Settings::default(),
            &DisplayStatus::default(),
        );
        assert_eq!(
            rebind.hit_test([48.0, 80.0]),
            None,
            "cannot click behind rebind"
        );
        assert_eq!(
            rebind.hit_test([48.0, 180.0]).map(|hit| hit.intent),
            Some(MenuIntent::CancelRebind),
            "rebind cancel"
        );
        ui.rebinding_action = None;
        ui.loading_map = true;
        let loading = menu_layout(
            AppState::LevelSelect,
            &ui,
            &Settings::default(),
            &DisplayStatus::default(),
        );
        assert_eq!(
            loading.hit_test([48.0, 80.0]),
            None,
            "cannot click behind load"
        );
        assert_eq!(
            loading.hit_test([48.0, 180.0]).map(|hit| hit.intent),
            Some(MenuIntent::CancelLoad),
            "load cancel"
        );
    }
}

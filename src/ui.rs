use crate::font::{get_char_uv, get_white_uv};
use crate::game::AppState;
use crate::render::Vertex;
use crate::settings::{KeyBindings, Settings};

/// Menu selection state across screens.
#[derive(Debug, Clone)]
pub struct UiState {
    pub main_menu_idx: usize,
    pub level_select_idx: usize,
    pub pause_menu_idx: usize,
    pub settings_idx: usize,
    pub rebinding_action: Option<&'static str>,
    pub status_message: Option<String>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            main_menu_idx: 0,
            level_select_idx: 0,
            pause_menu_idx: 0,
            settings_idx: 0,
            rebinding_action: None,
            status_message: None,
        }
    }
}

impl UiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel_rebinding(&mut self) {
        self.rebinding_action = None;
        self.status_message = None;
    }
}

fn add_ui_quad(
    vertices: &mut Vec<Vertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 3],
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
    });
    vertices.push(Vertex {
        pos: p1,
        color,
        uv: [u1, v0],
    });
    vertices.push(Vertex {
        pos: p2,
        color,
        uv: [u1, v1],
    });
    vertices.push(Vertex {
        pos: p0,
        color,
        uv: [u0, v0],
    });
    vertices.push(Vertex {
        pos: p2,
        color,
        uv: [u1, v1],
    });
    vertices.push(Vertex {
        pos: p3,
        color,
        uv: [u0, v1],
    });
}

pub fn add_rect(vertices: &mut Vec<Vertex>, x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 3]) {
    add_ui_quad(vertices, x0, y0, x1, y1, color, get_white_uv());
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
        if let Some(uv) = get_char_uv(ch) {
            if ch != ' ' {
                add_ui_quad(vertices, x, y, x + char_w, y + char_h, color, uv);
            }
        }
        x += char_w;
    }
}

/// Generates 2D UI overlay vertices in 480x272 pixel space for the active AppState.
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
        AppState::MainMenu => {
            // Dark scrim background panel
            add_rect(&mut vertices, 20.0, 20.0, 460.0, 252.0, [0.08, 0.08, 0.07]);
            add_rect(&mut vertices, 22.0, 22.0, 458.0, 250.0, [0.12, 0.11, 0.10]);

            // Title
            draw_text(
                &mut vertices,
                "LIMINAL",
                40.0,
                38.0,
                2.0,
                [0.92, 0.88, 0.45],
            );
            draw_text(
                &mut vertices,
                "PocketCHIP Walking Experience",
                40.0,
                58.0,
                1.0,
                [0.65, 0.65, 0.60],
            );

            // Menu Items
            let items = ["Level Select", "Settings", "Exit"];
            let start_y = 95.0;
            let line_h = 24.0;

            for (i, &item) in items.iter().enumerate() {
                let y = start_y + (i as f32) * line_h;
                let is_sel = i == ui_state.main_menu_idx;

                if is_sel {
                    add_rect(
                        &mut vertices,
                        38.0,
                        y - 2.0,
                        260.0,
                        y + 14.0,
                        [0.25, 0.23, 0.16],
                    );
                    let line = format!("> {item}");
                    draw_text(&mut vertices, &line, 40.0, y, 1.0, [1.0, 0.95, 0.40]);
                } else {
                    let line = format!("  {item}");
                    draw_text(&mut vertices, &line, 40.0, y, 1.0, [0.85, 0.85, 0.80]);
                }
            }

            // Version bottom-left (requirement 3)
            let ver_text = format!("v{version}");
            draw_text(&mut vertices, &ver_text, 35.0, 235.0, 1.0, [0.5, 0.5, 0.5]);

            // Controls help bottom
            draw_text(
                &mut vertices,
                "W/Z: Move   ENTER: Select",
                250.0,
                235.0,
                1.0,
                [0.55, 0.55, 0.50],
            );
        }
        AppState::LevelSelect => {
            add_rect(&mut vertices, 20.0, 20.0, 460.0, 252.0, [0.08, 0.08, 0.07]);
            add_rect(&mut vertices, 22.0, 22.0, 458.0, 250.0, [0.12, 0.11, 0.10]);

            draw_text(
                &mut vertices,
                "LEVEL SELECT",
                40.0,
                38.0,
                2.0,
                [0.92, 0.88, 0.45],
            );

            let items = [
                ("Level 1", true),
                ("Under construction", false),
                ("Under construction", false),
                ("Under construction", false),
                ("Back", true),
            ];

            let start_y = 80.0;
            let line_h = 22.0;

            for (i, &(label, enabled)) in items.iter().enumerate() {
                let y = start_y + (i as f32) * line_h;
                let is_sel = i == ui_state.level_select_idx;

                if is_sel {
                    add_rect(
                        &mut vertices,
                        38.0,
                        y - 2.0,
                        300.0,
                        y + 14.0,
                        [0.25, 0.23, 0.16],
                    );
                    let line = format!("> {label}");
                    let col = if enabled {
                        [1.0, 0.95, 0.40]
                    } else {
                        [0.55, 0.50, 0.35]
                    };
                    draw_text(&mut vertices, &line, 40.0, y, 1.0, col);
                } else {
                    let line = format!("  {label}");
                    let col = if enabled {
                        [0.85, 0.85, 0.80]
                    } else {
                        [0.45, 0.45, 0.42]
                    };
                    draw_text(&mut vertices, &line, 40.0, y, 1.0, col);
                }
            }

            draw_text(
                &mut vertices,
                "W/Z: Move   ENTER: Select   ESC: Back",
                160.0,
                235.0,
                1.0,
                [0.55, 0.55, 0.50],
            );
        }
        AppState::Paused => {
            // Semi-transparent pause scrim
            add_rect(&mut vertices, 80.0, 40.0, 400.0, 230.0, [0.06, 0.06, 0.05]);
            add_rect(&mut vertices, 82.0, 42.0, 398.0, 228.0, [0.12, 0.11, 0.10]);

            draw_text(
                &mut vertices,
                "PAUSED",
                100.0,
                58.0,
                2.0,
                [0.92, 0.88, 0.45],
            );

            let items = ["Resume", "Settings", "Return to Main Menu"];
            let start_y = 105.0;
            let line_h = 24.0;

            for (i, &item) in items.iter().enumerate() {
                let y = start_y + (i as f32) * line_h;
                let is_sel = i == ui_state.pause_menu_idx;

                if is_sel {
                    add_rect(
                        &mut vertices,
                        98.0,
                        y - 2.0,
                        340.0,
                        y + 14.0,
                        [0.25, 0.23, 0.16],
                    );
                    let line = format!("> {item}");
                    draw_text(&mut vertices, &line, 100.0, y, 1.0, [1.0, 0.95, 0.40]);
                } else {
                    let line = format!("  {item}");
                    draw_text(&mut vertices, &line, 100.0, y, 1.0, [0.85, 0.85, 0.80]);
                }
            }

            draw_text(
                &mut vertices,
                "ESC: Resume   ENTER: Select",
                120.0,
                205.0,
                1.0,
                [0.55, 0.55, 0.50],
            );
        }
        AppState::Settings | AppState::PauseSettings => {
            add_rect(&mut vertices, 10.0, 10.0, 470.0, 262.0, [0.08, 0.08, 0.07]);
            add_rect(&mut vertices, 12.0, 12.0, 468.0, 260.0, [0.12, 0.11, 0.10]);

            draw_text(
                &mut vertices,
                "SETTINGS",
                25.0,
                20.0,
                2.0,
                [0.92, 0.88, 0.45],
            );

            // Rebinding prompt or conflict message
            if let Some(action) = ui_state.rebinding_action {
                add_rect(&mut vertices, 160.0, 18.0, 460.0, 36.0, [0.35, 0.15, 0.10]);
                let prompt = format!("PRESS KEY FOR {action} (ESC: cancel)");
                draw_text(&mut vertices, &prompt, 165.0, 22.0, 1.0, [1.0, 0.9, 0.3]);
            } else if let Some(ref msg) = ui_state.status_message {
                let col = if msg.starts_with("Error") || msg.contains("already") {
                    [1.0, 0.4, 0.3]
                } else {
                    [0.4, 0.9, 0.4]
                };
                draw_text(&mut vertices, msg, 160.0, 22.0, 1.0, col);
            }

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
                "Restore Default Bindings".to_string(),
                "Back".to_string(),
            ];

            let start_y = 44.0;
            let line_h = 13.0;

            for (i, label) in items.iter().enumerate() {
                let y = start_y + (i as f32) * line_h;
                let is_sel = i == ui_state.settings_idx;

                if is_sel {
                    add_rect(
                        &mut vertices,
                        23.0,
                        y - 1.0,
                        450.0,
                        y + 10.0,
                        [0.25, 0.23, 0.16],
                    );
                    let line = format!("> {label}");
                    draw_text(&mut vertices, &line, 25.0, y, 1.0, [1.0, 0.95, 0.40]);
                } else {
                    let line = format!("  {label}");
                    draw_text(&mut vertices, &line, 25.0, y, 1.0, [0.85, 0.85, 0.80]);
                }
            }

            draw_text(
                &mut vertices,
                "W/Z: Nav  ENTER/A/S: Adjust/Rebind  ESC: Back",
                70.0,
                250.0,
                1.0,
                [0.55, 0.55, 0.50],
            );
        }
    }

    vertices
}

pub const SETTINGS_ITEM_COUNT: usize = 16;
pub const SETTINGS_ACTIONS: [&str; 8] = [
    "forward",
    "strafe_left",
    "strafe_right",
    "backward",
    "look_up",
    "look_down",
    "look_left",
    "look_right",
];

/// Cycles or rebinds selected settings item.
pub fn activate_settings_item(
    idx: usize,
    ui_state: &mut UiState,
    settings: &mut Settings,
    direction: i32,
) -> bool {
    // 0..8: Keybindings
    if idx < 8 {
        ui_state.rebinding_action = Some(SETTINGS_ACTIONS[idx]);
        ui_state.status_message = None;
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
            ui_state.status_message = Some("Horizontal look speed updated".to_string());
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
            ui_state.status_message = Some("Vertical look speed updated".to_string());
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
            ui_state.status_message = Some("Walk speed updated".to_string());
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
            ui_state.status_message = Some("FOV updated".to_string());
        }
        12 => {
            // VSync
            settings.vsync = !settings.vsync;
            ui_state.status_message = Some(format!(
                "VSync {}",
                if settings.vsync {
                    "enabled"
                } else {
                    "disabled"
                }
            ));
        }
        13 => {
            // Filtering
            settings.texture_filtering = if settings.texture_filtering == "linear" {
                "nearest".to_string()
            } else {
                "linear".to_string()
            };
            ui_state.status_message = Some(format!(
                "Texture filtering: {}",
                settings.texture_filtering.to_uppercase()
            ));
        }
        14 => {
            // Restore defaults
            settings.bindings = KeyBindings::default();
            settings.look_speed_h = 90.0;
            settings.look_speed_v = 60.0;
            settings.walk_speed = 3.0;
            settings.fov_degrees = 60.0;
            settings.vsync = true;
            settings.texture_filtering = "linear".to_string();
            ui_state.status_message = Some("Restored default settings & bindings".to_string());
        }
        15 => {
            // Back
            return true; // Signal back
        }
        _ => {}
    }
    let _ = settings.save();
    false
}

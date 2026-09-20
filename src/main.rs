pub mod collision;
pub mod font;
pub mod game;
pub mod input;
pub mod level;
pub mod perf;
pub mod render;
pub mod settings;
pub mod ui;

use glam::Vec3;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use game::{AppState, Game};
use input::{InputHandler, MenuNavEvent, keycode_to_str};
use level::LevelDef;
use perf::PerfOverlay;
use render::{Renderer, WINDOW_HEIGHT, WINDOW_WIDTH};
use settings::Settings;
use ui::{SETTINGS_ITEM_COUNT, UiState, activate_settings_item, build_ui_geometry};

const LEVEL1_JSON: &str = include_str!("../assets/levels/level1.json");
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdl_context = sdl2::init().map_err(|e| format!("Failed to init SDL2: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    // Load persisted settings or fallback safely to PocketCHIP defaults
    let mut settings = Settings::load_or_default();

    // Open a 480x272 game window suitable for the PocketCHIP target
    let window = video_subsystem
        .window("Liminal", WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| format!("Failed to create window: {e}"))?;

    // Apply VSync from settings
    let _ = video_subsystem.gl_set_swap_interval(if settings.vsync {
        sdl2::video::SwapInterval::VSync
    } else {
        sdl2::video::SwapInterval::Immediate
    });

    // Load Level 1 layout from data-driven JSON format
    let level = LevelDef::from_json(LEVEL1_JSON)
        .map_err(|e| format!("Failed to parse Level 1 JSON: {e}"))?;

    let renderer = Renderer::new(&window, &video_subsystem, &level)
        .map_err(|e| format!("Failed to initialize renderer: {e}"))?;

    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| format!("Failed to init event pump: {e}"))?;

    let mut input_handler = InputHandler::new();
    let spawn_pos = Vec3::new(level.spawn.x, 1.6, level.spawn.z);
    let spawn_yaw = level.spawn.yaw_degrees.to_radians();
    let mut game = Game::new(spawn_pos, spawn_yaw, level.collision_aabbs());
    let mut ui_state = UiState::new();
    let mut perf_overlay = PerfOverlay::new();

    // Clean main loop
    while game.is_running() {
        game.update_timing();
        perf_overlay.update(game.delta_seconds());

        for event in event_pump.poll_iter() {
            if let Event::Quit { .. } = event {
                game.stop();
                break;
            }

            let app_state = game.app_state();

            // 1. If currently waiting for key rebinding in Settings
            if let Some(action) = ui_state.rebinding_action {
                if let Event::KeyDown {
                    keycode: Some(key), ..
                } = event
                {
                    if key == Keycode::Escape {
                        ui_state.cancel_rebinding();
                    } else {
                        let key_str = keycode_to_str(key);
                        match settings.bindings.set_key(action, &key_str) {
                            Ok(()) => {
                                ui_state.status_message =
                                    Some(format!("Bound {action} to [{key_str}]"));
                                let _ = settings.save();
                            }
                            Err(err) => {
                                ui_state.status_message = Some(err);
                            }
                        }
                        ui_state.rebinding_action = None;
                    }
                }
                continue;
            }

            // Performance overlay toggle with '-' key (hidden by default)
            if let Event::KeyDown {
                keycode: Some(Keycode::Minus | Keycode::KpMinus),
                repeat: false,
                ..
            } = event
            {
                perf_overlay.toggle();
                input_handler.set_overlay_visible(perf_overlay.is_visible());
                continue;
            }

            // 2. Menu navigation for non-playing states (independent of gameplay bindings)
            if app_state != AppState::Playing {
                if let Some(nav) = InputHandler::poll_menu_nav_event(&event) {
                    match nav {
                        MenuNavEvent::Up => match app_state {
                            AppState::MainMenu => {
                                ui_state.main_menu_idx = (ui_state.main_menu_idx + 3 - 1) % 3;
                            }
                            AppState::LevelSelect => {
                                ui_state.level_select_idx = (ui_state.level_select_idx + 5 - 1) % 5;
                            }
                            AppState::Paused => {
                                ui_state.pause_menu_idx = (ui_state.pause_menu_idx + 3 - 1) % 3;
                            }
                            AppState::Settings | AppState::PauseSettings => {
                                ui_state.settings_idx =
                                    (ui_state.settings_idx + SETTINGS_ITEM_COUNT - 1)
                                        % SETTINGS_ITEM_COUNT;
                            }
                            _ => {}
                        },
                        MenuNavEvent::Down => match app_state {
                            AppState::MainMenu => {
                                ui_state.main_menu_idx = (ui_state.main_menu_idx + 1) % 3;
                            }
                            AppState::LevelSelect => {
                                ui_state.level_select_idx = (ui_state.level_select_idx + 1) % 5;
                            }
                            AppState::Paused => {
                                ui_state.pause_menu_idx = (ui_state.pause_menu_idx + 1) % 3;
                            }
                            AppState::Settings | AppState::PauseSettings => {
                                ui_state.settings_idx =
                                    (ui_state.settings_idx + 1) % SETTINGS_ITEM_COUNT;
                            }
                            _ => {}
                        },
                        MenuNavEvent::Left => {
                            if app_state == AppState::Settings
                                || app_state == AppState::PauseSettings
                            {
                                activate_settings_item(
                                    ui_state.settings_idx,
                                    &mut ui_state,
                                    &mut settings,
                                    -1,
                                );
                            }
                        }
                        MenuNavEvent::Right => {
                            if app_state == AppState::Settings
                                || app_state == AppState::PauseSettings
                            {
                                activate_settings_item(
                                    ui_state.settings_idx,
                                    &mut ui_state,
                                    &mut settings,
                                    1,
                                );
                            }
                        }
                        MenuNavEvent::Activate => match app_state {
                            AppState::MainMenu => match ui_state.main_menu_idx {
                                0 => game.set_app_state(AppState::LevelSelect),
                                1 => game.set_app_state(AppState::Settings),
                                2 => game.stop(),
                                _ => {}
                            },
                            AppState::LevelSelect => match ui_state.level_select_idx {
                                0 => {
                                    // Launch Level 1
                                    input_handler.clear_gameplay_inputs();
                                    game.set_app_state(AppState::Playing);
                                }
                                4 => game.set_app_state(AppState::MainMenu),
                                _ => {} // Under construction (disabled)
                            },
                            AppState::Paused => match ui_state.pause_menu_idx {
                                0 => {
                                    input_handler.clear_gameplay_inputs();
                                    game.set_app_state(AppState::Playing);
                                }
                                1 => game.set_app_state(AppState::PauseSettings),
                                2 => game.set_app_state(AppState::MainMenu),
                                _ => {}
                            },
                            AppState::Settings => {
                                if activate_settings_item(
                                    ui_state.settings_idx,
                                    &mut ui_state,
                                    &mut settings,
                                    1,
                                ) {
                                    game.set_app_state(AppState::MainMenu);
                                }
                            }
                            AppState::PauseSettings => {
                                if activate_settings_item(
                                    ui_state.settings_idx,
                                    &mut ui_state,
                                    &mut settings,
                                    1,
                                ) {
                                    game.set_app_state(AppState::Paused);
                                }
                            }
                            _ => {}
                        },
                        MenuNavEvent::Back => {
                            game.handle_escape();
                        }
                    }
                }
            } else {
                // 3. Gameplay active (AppState::Playing)
                if let Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    ..
                } = event
                {
                    input_handler.clear_gameplay_inputs();
                    game.handle_escape(); // Opens Pause menu
                } else {
                    input_handler.handle_gameplay_event(&event, &settings.bindings);
                }
            }
        }

        if input_handler.quit_requested() {
            game.stop();
        }

        // Update player movement (only active during AppState::Playing)
        game.update_player_movement(input_handler.state(), &settings);

        // Render scene
        let (cam_pos, cam_yaw, cam_pitch) = match game.app_state() {
            AppState::Playing | AppState::Paused | AppState::PauseSettings => {
                (game.player_position, game.player_yaw, game.player_pitch)
            }
            AppState::MainMenu | AppState::LevelSelect | AppState::Settings => {
                // Static menu background camera
                (spawn_pos, spawn_yaw, 0.0)
            }
        };

        renderer.render_scene(
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            cam_pos,
            cam_yaw,
            cam_pitch,
            settings.fov_degrees,
        );

        // Render UI overlay on top if in a menu or pause state, plus performance overlay if visible
        let mut ui_vertices = build_ui_geometry(game.app_state(), &ui_state, &settings, APP_VERSION);
        if perf_overlay.is_visible() {
            ui_vertices.extend_from_slice(perf_overlay.cached_vertices());
        }
        renderer.render_ui(WINDOW_WIDTH, WINDOW_HEIGHT, &ui_vertices);

        // Swap window buffer (double buffered, VSync synchronized)
        window.gl_swap_window();
    }

    Ok(())
}

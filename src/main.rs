pub mod collision;
pub mod font;
pub mod game;
pub mod input;
pub mod level;
pub mod loader;
pub mod perf;
pub mod render;
pub mod settings;
pub mod ui;

use glam::Vec3;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use game::{AppState, Game};
use input::{InputHandler, MenuNavEvent, keycode_to_str};
use perf::PerfOverlay;
use render::{DrawableSize, Renderer, Vertex, WINDOW_HEIGHT, WINDOW_WIDTH};
use settings::Settings;
use ui::{SETTINGS_ITEM_COUNT, UiGeometryCache, UiState, activate_settings_item};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Configures SDL OpenGL attributes before the window/context is created.
///
/// When `gles` is true, requests an OpenGL ES 2.0 context (PocketCHIP baseline);
/// otherwise the platform default profile is used so desktop development still
/// works. Double buffering is always requested.
fn configure_gl_attributes(video: &sdl2::VideoSubsystem, gles: bool) {
    let attr = video.gl_attr();
    attr.set_double_buffer(true);
    attr.set_depth_size(24);
    if gles {
        attr.set_context_profile(sdl2::video::GLProfile::GLES);
        attr.set_context_version(2, 0);
    } else {
        // Reset the profile for the fallback path: SDL GL attributes are sticky,
        // so the ES request must be explicitly overridden or the retry would
        // fail identically.
        attr.set_context_profile(sdl2::video::GLProfile::Compatibility);
        attr.set_context_version(2, 1);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sdl_context = sdl2::init().map_err(|e| format!("Failed to init SDL2: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    // SDL enables Unicode text input (and therefore the platform IME) implicitly
    // as soon as the video subsystem starts. This game only consumes raw key
    // events, never composed text, so disable it: on macOS this is the code path
    // that engages InputMethodKit (`interpretKeyEvents` on SDL's text responder).
    video_subsystem.text_input().stop();

    // Load persisted settings or fallback safely to PocketCHIP defaults
    let mut settings = Settings::load_or_default();

    // Configure the framebuffer/context attributes *before* the OpenGL window
    // is created. On Linux/EGL (PocketCHIP) the visual (depth buffer, double
    // buffering) is chosen at window creation, so setting these afterwards
    // would have no effect.
    configure_gl_attributes(&video_subsystem, true);

    // Open a game window sized to the PocketCHIP baseline; it is resizable so it
    // can use the full drawable on desktop and HiDPI displays. If an OpenGL ES
    // window cannot be created (e.g. desktop macOS), fall back to the platform
    // default profile so development builds still run.
    let build_window = || {
        video_subsystem
            .window("Liminal", WINDOW_WIDTH, WINDOW_HEIGHT)
            .position_centered()
            .resizable()
            .allow_highdpi()
            .opengl()
            .build()
    };
    let window = match build_window() {
        Ok(window) => window,
        Err(_) => {
            configure_gl_attributes(&video_subsystem, false);
            build_window().map_err(|e| format!("Failed to create window: {e}"))?
        }
    };

    // Apply VSync from settings
    let _ = video_subsystem.gl_set_swap_interval(if settings.vsync {
        sdl2::video::SwapInterval::VSync
    } else {
        sdl2::video::SwapInterval::Immediate
    });

    let mut level_manager = loader::LevelManager::new();
    let initial_level = level_manager
        .load_default_or_level1()
        .map_err(|e| format!("Failed to load initial level: {e}"))?;

    let mut renderer = Renderer::new(&window, &video_subsystem, &initial_level.level)
        .map_err(|e| format!("Failed to initialize renderer: {e}"))?;
    renderer.set_level(&initial_level);
    renderer.set_texture_filtering(&settings.texture_filtering);

    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| format!("Failed to init event pump: {e}"))?;

    let mut input_handler = InputHandler::new();
    let mut spawn_pos = Vec3::new(
        initial_level.level.spawn.x,
        1.6,
        initial_level.level.spawn.z,
    );
    let mut spawn_yaw = initial_level.level.spawn.yaw_degrees.to_radians();
    let mut game = Game::new(spawn_pos, spawn_yaw, initial_level.level.collision_aabbs());
    // All required level state has been extracted (spawn, collision walls,
    // renderer uploads), so release the CPU-side textures and level definition.
    drop(initial_level);
    let mut ui_state = UiState::new();
    ui_state.level_entries = level_manager
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect();
    let mut perf_overlay = PerfOverlay::new();
    let mut ui_cache = UiGeometryCache::new();
    let mut ui_scratch: Vec<Vertex> = Vec::new();
    let mut applied_filtering = settings.texture_filtering.clone();

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
                    let level_items_count = ui_state.level_entries.len() + 2; // levels + Load/Import + Back
                    match nav {
                        MenuNavEvent::Up => match app_state {
                            AppState::MainMenu => {
                                ui_state.main_menu_idx = (ui_state.main_menu_idx + 3 - 1) % 3;
                            }
                            AppState::LevelSelect => {
                                ui_state.level_select_idx =
                                    (ui_state.level_select_idx + level_items_count - 1)
                                        % level_items_count;
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
                                ui_state.level_select_idx =
                                    (ui_state.level_select_idx + 1) % level_items_count;
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
                                0 => {
                                    // Discovery metadata is cached at startup and
                                    // refreshed after imports; opening the level
                                    // list must not re-scan/re-extract packs.
                                    ui_state.level_entries = level_manager
                                        .entries()
                                        .iter()
                                        .map(|e| e.name.clone())
                                        .collect();
                                    game.set_app_state(AppState::LevelSelect);
                                }
                                1 => game.set_app_state(AppState::Settings),
                                2 => game.stop(),
                                _ => {}
                            },
                            AppState::LevelSelect => {
                                let num_levels = level_manager.entries().len();
                                if ui_state.level_select_idx < num_levels {
                                    if let Some(entry) =
                                        level_manager.get_entry(ui_state.level_select_idx)
                                    {
                                        match level_manager.load_level(entry) {
                                            Ok(loaded) => {
                                                renderer.set_level(&loaded);
                                                spawn_pos = Vec3::new(
                                                    loaded.level.spawn.x,
                                                    1.6,
                                                    loaded.level.spawn.z,
                                                );
                                                spawn_yaw =
                                                    loaded.level.spawn.yaw_degrees.to_radians();
                                                game.reset_level(
                                                    spawn_pos,
                                                    spawn_yaw,
                                                    loaded.level.collision_aabbs(),
                                                );
                                                input_handler.clear_gameplay_inputs();
                                                ui_state.status_message = None;
                                                game.set_app_state(AppState::Playing);
                                            }
                                            Err(err) => {
                                                ui_state.status_message =
                                                    Some(format!("Load failed: {err}"));
                                            }
                                        }
                                    }
                                } else if ui_state.level_select_idx == num_levels {
                                    // Load/Import Level
                                    match level_manager.import_available() {
                                        Ok(count) => {
                                            // `import_available` rescans after importing.
                                            ui_state.level_entries = level_manager
                                                .entries()
                                                .iter()
                                                .map(|e| e.name.clone())
                                                .collect();
                                            if count > 0 {
                                                ui_state.status_message = Some(format!(
                                                    "Imported {count} level(s) from import/"
                                                ));
                                            } else {
                                                ui_state.status_message = Some(
                                                    "No new .json/.zip in import/ or levels/import/"
                                                        .to_string(),
                                                );
                                            }
                                        }
                                        Err(err) => {
                                            ui_state.status_message =
                                                Some(format!("Import error: {err}"));
                                        }
                                    }
                                } else {
                                    // Back
                                    game.set_app_state(AppState::MainMenu);
                                }
                            }
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
                            AppState::PauseSettings
                                if activate_settings_item(
                                    ui_state.settings_idx,
                                    &mut ui_state,
                                    &mut settings,
                                    1,
                                ) =>
                            {
                                game.set_app_state(AppState::Paused);
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

        // Use the physical drawable size, not the logical window size, so HiDPI
        // (Retina) backing scale and monitor changes are handled automatically.
        let (drawable_width, drawable_height) = window.drawable_size();
        let drawable = DrawableSize::new(drawable_width, drawable_height);

        // Minimized/hidden windows report a zero-sized drawable. Skip rendering to
        // avoid invalid GL state and keep timing fresh so restoring does not jump.
        if drawable.is_empty() {
            game.reset_timing();
            std::thread::sleep(std::time::Duration::from_millis(16));
            continue;
        }

        renderer.set_drawable_size(drawable);

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

        renderer.render_scene(cam_pos, cam_yaw, cam_pitch, settings.fov_degrees);

        // Apply a changed texture filtering setting to existing GL textures
        // without re-uploading their pixel data.
        if settings.texture_filtering != applied_filtering {
            renderer.set_texture_filtering(&settings.texture_filtering);
            applied_filtering.clone_from(&settings.texture_filtering);
        }

        // Menu/settings UI geometry is cached and only rebuilt when its inputs
        // change. The (debug) performance overlay is appended on demand.
        let ui_vertices = ui_cache.get(game.app_state(), &ui_state, &settings, APP_VERSION);
        if perf_overlay.is_visible() {
            ui_scratch.clear();
            ui_scratch.extend_from_slice(ui_vertices);
            ui_scratch.extend_from_slice(perf_overlay.cached_vertices());
            renderer.render_ui(&ui_scratch);
        } else {
            renderer.render_ui(ui_vertices);
        }

        // Swap window buffer (double buffered, VSync synchronized)
        window.gl_swap_window();
    }

    Ok(())
}

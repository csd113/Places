pub mod bench;
pub mod collision;
pub mod font;
pub mod game;
pub mod gltf;
pub mod input;
pub mod level;
pub mod lighting;
#[cfg(test)]
mod lighting_audit;
#[cfg(test)]
mod lighting_audit_cases;
#[cfg(test)]
mod lighting_parity;
pub mod loader;
pub mod perf;
pub mod props;
pub mod render;
pub mod settings;
pub mod spatial;
#[cfg(test)]
mod test_support;
pub mod ui;

use std::cmp::Ordering;

use glam::Vec3;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use bench::Bench;
use game::{AppState, Game};
use input::{InputHandler, MenuNavEvent, keycode_to_str};
use perf::PerfOverlay;
use render::{DrawableSize, Renderer, Vertex, WINDOW_HEIGHT, WINDOW_WIDTH};
use settings::Settings;
use ui::{SETTINGS_ITEM_COUNT, UiGeometryCache, UiState, activate_settings_item};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Prints what the level's props and baked lighting cost, so hardware runs
/// (`PocketCHIP` over SSH) can be checked without a debugger: decoded models,
/// texture memory, draw calls, level-build time and the baked room baselines.
fn log_prop_usage(renderer: &Renderer) {
    let stats = renderer.prop_asset_stats();
    println!(
        "[props] {} models cached ({} failed), {} triangles, {} KiB of textures, {} draw call(s)",
        stats.models_loaded,
        stats.models_failed,
        stats.triangles,
        stats.texture_bytes / 1024,
        renderer.prop_draw_count()
    );
    let level = renderer.level_stats();
    let batches = renderer.static_batch_family_breakdown();
    println!(
        "[level] {} static vertices, {} prop vertices, {} prop draw call(s), built in {:.1} ms \
         (lighting {:.1} + props {:.1} + surfaces {:.1})",
        level.static_vertices,
        level.prop_vertices,
        level.prop_draws,
        level.build_millis,
        level.lighting_millis,
        level.props_millis,
        level.surfaces_millis
    );
    println!(
        "[spatial] {} cells: {} static batch(es) (floor {} / ceiling {} / wall {} / light {} / prop box {} / decal {}), {} prop batch(es)",
        renderer.spatial_grid().describe(),
        renderer.static_batch_count(),
        batches[0],
        batches[1],
        batches[2],
        batches[3],
        batches[4],
        batches[5],
        level.prop_draws
    );
    println!(
        "[lighting] baked {} room(s) from {} fixture(s): baselines {:.2}..{:.2} (avg {:.2})",
        level.lighting.rooms,
        level.lighting.lights,
        level.lighting.min_baseline,
        level.lighting.max_baseline,
        level.lighting.average_baseline
    );
}

/// Parses `LIMINAL_SPAWN` overrides: `x,z,yaw_degrees` keeps the default eye
/// height, `x,y,z,yaw_degrees` sets it explicitly. Invalid input is ignored.
fn parse_spawn_override(value: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = value
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<f32>>>()?;
    let numbers: Vec<f32> = parts
        .iter()
        .copied()
        .filter(|number| number.is_finite())
        .collect();
    match numbers.len() {
        3 => Some([numbers[0], 1.6, numbers[1], numbers[2]]),
        4 => Some([numbers[0], numbers[1], numbers[2], numbers[3]]),
        _ => None,
    }
}

/// Configures SDL OpenGL attributes before the window/context is created.
///
/// When `gles` is true, requests an OpenGL ES 2.0 context (`PocketCHIP` baseline);
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

/// Requests a swap interval and reports what the platform actually accepted.
///
/// This used to discard the result of `SDL_GL_SetSwapInterval`, which made a
/// silently ignored `VSync` request indistinguishable from a working one. The
/// requested interval, the call's return status and `SDL_GL_GetSwapInterval`
/// (a fresh query of the platform, not an echo of the request) are all logged
/// once at startup, and the interval in force is returned for the caller.
fn apply_swap_interval(video: &sdl2::VideoSubsystem, want_vsync: bool) -> i32 {
    let requested = if want_vsync {
        sdl2::video::SwapInterval::VSync
    } else {
        sdl2::video::SwapInterval::Immediate
    };
    match video.gl_set_swap_interval(requested) {
        Ok(()) => {
            let reported = video.gl_get_swap_interval();
            println!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Ok, SDL_GL_GetSwapInterval -> {reported:?}",
            );
            reported as i32
        }
        Err(error) => {
            let reported = video.gl_get_swap_interval();
            println!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Err({error}), SDL_GL_GetSwapInterval -> {reported:?}",
            );
            reported as i32
        }
    }
}

/// Root of an installed legacy package.
///
/// A deployed native payload is launched from `bin/<target-triple>/app`, three
/// levels below its package root. A development build falls back to the crate
/// path so tests and `cargo run` use the repository root.
fn package_root() -> std::path::PathBuf {
    let fallback = || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Ok(executable) = std::env::current_exe() else {
        return fallback();
    };
    let installed = executable
        .file_name()
        .is_some_and(|name| name == "app")
        .then(|| {
            executable
                .ancestors()
                .nth(3)
                .map(std::path::Path::to_path_buf)
        })
        .flatten();
    match installed {
        Some(root) if root.join("assets/levels").is_dir() => root,
        _ => fallback(),
    }
}

/// Points every relative asset path at the installed package.
///
/// The level loader, the prop catalogue, imported level packs and the settings
/// file are all resolved relative to the working directory, so an installed
/// package has to run from its own root. A development build already satisfies
/// this and is left alone.
fn use_package_assets() -> std::path::PathBuf {
    let package = package_root();
    let current = std::env::current_dir().ok();
    if current.as_deref() != Some(package.as_path())
        && let Err(error) = std::env::set_current_dir(&package)
    {
        eprintln!(
            "could not use the installed package directory {}: {error}",
            package.display()
        );
    }
    package
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = use_package_assets();
    println!(
        "[package] {} (assets: {})",
        package.display(),
        package.join("assets/levels").display()
    );
    // X11 process identity, required for the App Center launcher and window
    // managers to associate the window with this app.
    sdl2::hint::set("SDL_VIDEO_X11_WMCLASS", "io.vitrallis.liminalrust");
    sdl2::hint::set("SDL_APP_NAME", "Liminal");

    let sdl_context = sdl2::init().map_err(|e| format!("Failed to init SDL2: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    // SDL enables Unicode text input (and therefore the platform IME) implicitly
    // as soon as the video subsystem starts. This game only consumes raw key
    // events, never composed text, so disable it: on macOS this is the code path
    // that engages InputMethodKit (`interpretKeyEvents` on SDL's text responder).
    video_subsystem.text_input().stop();

    // Load persisted settings or fallback safely to the default settings
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
    let window = if let Ok(window) = build_window() {
        window
    } else {
        configure_gl_attributes(&video_subsystem, false);
        build_window().map_err(|e| format!("Failed to create window: {e}"))?
    };

    // Debug-only frame telemetry. Inert unless `LIMINAL_BENCH=1` is set.
    let mut bench = Bench::new();

    let mut level_manager = loader::LevelManager::new();
    let initial_level = level_manager
        .load_default_or_level1()
        .map_err(|e| format!("Failed to load initial level: {e}"))?;

    let mut renderer = Renderer::new(&window, &video_subsystem)
        .map_err(|e| format!("Failed to initialize renderer: {e}"))?;
    renderer.set_level(&initial_level);
    renderer.set_texture_filtering(&settings.texture_filtering);
    // The three benchmark switches below keep the level build, the batching, the
    // draw order and the shader identical and change exactly one submission
    // decision each, which is how a single build measures what culling, indexing
    // and vertex packing are each worth on real hardware.
    renderer.set_culling(!bench.no_cull());
    renderer.set_indexing(!bench.no_index());
    renderer.set_vertex_layout(if bench.exact_vertex() {
        render::VertexLayout::Exact
    } else {
        render::VertexLayout::Packed
    });

    // Apply VSync from settings *after* the GL context exists and is current.
    // `SDL_GL_SetSwapInterval` fails outright without a current context, which is
    // why the request used to be dropped and the renderer's own unconditional
    // VSync-on call won instead. `LIMINAL_VSYNC=on|off` overrides this for VSync
    // characterisation runs only; the shipping default stays VSync-on.
    let want_vsync = bench.vsync_override().unwrap_or(settings.vsync);
    let swap_interval = apply_swap_interval(&video_subsystem, want_vsync);
    bench.set_reported_swap_interval(swap_interval);

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
    log_prop_usage(&renderer);
    // All required level state has been extracted (spawn, collision walls,
    // renderer uploads), so release the CPU-side textures and level definition.
    drop(initial_level);

    // Developer / hardware shortcut: boot straight into a level, which is how
    // the prop showcase and stress levels are checked on the PocketCHIP (where
    // the menu cannot be driven over SSH):
    //   LIMINAL_LEVEL=prop_showcase ./liminal-rust
    if let Some(requested) = std::env::var("LIMINAL_LEVEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let index = level_manager
            .entries()
            .iter()
            .position(|entry| entry.id == requested || entry.name.eq_ignore_ascii_case(&requested));
        match index.and_then(|index| level_manager.get_entry(index).cloned()) {
            Some(entry) => match level_manager.load_level(&entry) {
                Ok(loaded) => {
                    println!(
                        "LIMINAL_LEVEL: loading '{}' ({}) - {} props",
                        loaded.level.name,
                        loaded.level.id,
                        loaded.level.props.len()
                    );
                    renderer.set_level(&loaded);
                    renderer.set_culling(!bench.no_cull());
                    renderer.set_indexing(!bench.no_index());
                    log_prop_usage(&renderer);
                    spawn_pos = Vec3::new(loaded.level.spawn.x, 1.6, loaded.level.spawn.z);
                    spawn_yaw = loaded.level.spawn.yaw_degrees.to_radians();
                    game.reset_level(spawn_pos, spawn_yaw, loaded.level.collision_aabbs());
                    game.set_app_state(AppState::Playing);
                }
                Err(error) => {
                    eprintln!("LIMINAL_LEVEL: could not load '{requested}': {error}");
                }
            },
            None => eprintln!(
                "LIMINAL_LEVEL: no level matches '{requested}'; installed levels: {}",
                level_manager
                    .entries()
                    .iter()
                    .map(|entry| entry.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
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
    let mut capture_path: Option<std::path::PathBuf> = std::env::var("LIMINAL_CAPTURE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    // `LIMINAL_SPAWN=x,z,yaw_degrees` (or `x,y,z,yaw_degrees`) overrides the
    // level's spawn point, so a hardware run can stand in front of a specific
    // prop instead of walking there with a pad.
    let spawn_override: Option<[f32; 4]> = std::env::var("LIMINAL_SPAWN")
        .ok()
        .and_then(|value| parse_spawn_override(&value));
    if let Some([x, y, z, yaw]) = spawn_override {
        spawn_pos = Vec3::new(x, y, z);
        spawn_yaw = yaw.to_radians();
        game.reset_level(spawn_pos, spawn_yaw, game.walls.clone());
    }

    // Clean main loop
    while game.is_running() {
        // Frame boundary for the benchmark harness: everything from here to the
        // end of `gl_swap_window` is one complete frame, swap included.
        let frame_begin = std::time::Instant::now();
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
            if app_state == AppState::Playing {
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
            } else if let Some(nav) = InputHandler::poll_menu_nav_event(&event) {
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
                            ui_state.settings_idx = (ui_state.settings_idx + SETTINGS_ITEM_COUNT
                                - 1)
                                % SETTINGS_ITEM_COUNT;
                        }
                        AppState::Playing => {}
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
                        AppState::Playing => {}
                    },
                    MenuNavEvent::Left => {
                        if app_state == AppState::Settings || app_state == AppState::PauseSettings {
                            activate_settings_item(
                                ui_state.settings_idx,
                                &mut ui_state,
                                &mut settings,
                                -1,
                            );
                        }
                    }
                    MenuNavEvent::Right => {
                        if app_state == AppState::Settings || app_state == AppState::PauseSettings {
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
                            match ui_state.level_select_idx.cmp(&num_levels) {
                                // One of the installed levels: load it.
                                Ordering::Less => {
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
                                }
                                // `Load/Import Level`.
                                Ordering::Equal => match level_manager.import_available() {
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
                                },
                                // `Back`.
                                Ordering::Greater => {
                                    game.set_app_state(AppState::MainMenu);
                                }
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
        }

        if input_handler.quit_requested() {
            game.stop();
        }

        // Update player movement (only active during AppState::Playing)
        game.update_player_movement(input_handler.state(), &settings);
        let frame_update_done = std::time::Instant::now();

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
        // `LIMINAL_CAMERA=yaw[,pitch]` pins the camera so a hardware benchmark
        // measures the same view twice; it never changes gameplay.
        let (cam_yaw, cam_pitch) = match bench.camera_override() {
            Some((yaw, pitch)) => (yaw.to_radians(), pitch.to_radians()),
            None => (cam_yaw, cam_pitch),
        };

        let skip_render = bench.skip_render();
        if !skip_render {
            renderer.render_scene(cam_pos, cam_yaw, cam_pitch, settings.fov_degrees);
        }
        let frame_render_done = std::time::Instant::now();
        // Apply a changed texture filtering setting to existing GL textures
        // without re-uploading their pixel data.
        if settings.texture_filtering != applied_filtering {
            renderer.set_texture_filtering(&settings.texture_filtering);
            applied_filtering.clone_from(&settings.texture_filtering);
        }

        // Menu/settings UI geometry is cached and only rebuilt when its inputs
        // change. The (debug) performance overlay is appended on demand.
        let ui_vertices = ui_cache.get(game.app_state(), &ui_state, &settings, APP_VERSION);
        if skip_render {
            // `LIMINAL_BENCH_NORENDER=1`: measure the presentation path alone.
        } else if perf_overlay.is_visible() {
            ui_scratch.clear();
            ui_scratch.extend_from_slice(ui_vertices);
            ui_scratch.extend_from_slice(perf_overlay.cached_vertices());
            renderer.render_ui(&ui_scratch);
        } else {
            renderer.render_ui(ui_vertices);
        }
        // `LIMINAL_BENCH_FINISH=1`: force the GL pipeline to drain before the
        // swap timing point, so `render_ms` is renderer completion time rather
        // than "how much of the frame the driver happened to absorb".
        if bench.finish_before_swap() {
            renderer.finish();
        }
        let frame_ui_done = std::time::Instant::now();

        // Developer / hardware capture: `LIMINAL_CAPTURE=frame.png` renders one
        // frame of the running level and writes it out, which is how prop
        // rendering is inspected on the PocketCHIP over SSH (or on a desktop
        // where the window cannot be screenshotted).
        if let Some(path) = capture_path.as_ref() {
            match renderer.capture_default_framebuffer() {
                Ok(image) => match loader::encode_png(&image) {
                    Ok(bytes) => match std::fs::write(path, bytes) {
                        Ok(()) => println!("LIMINAL_CAPTURE: wrote {}", path.display()),
                        Err(error) => {
                            eprintln!("LIMINAL_CAPTURE: cannot write {}: {error}", path.display());
                        }
                    },
                    Err(error) => eprintln!("LIMINAL_CAPTURE: {error}"),
                },
                Err(error) => eprintln!("LIMINAL_CAPTURE: {error}"),
            }
            game.stop();
            capture_path = None;
        }

        // Swap window buffer (double buffered, VSync synchronized)
        if !bench.skip_swap() {
            window.gl_swap_window();
        }
        let frame_swap_done = std::time::Instant::now();

        if bench.enabled() {
            bench.record_frame(
                frame_begin,
                frame_update_done,
                frame_render_done,
                frame_ui_done,
                frame_swap_done,
                renderer.render_stats(),
            );
            if bench.is_complete() {
                bench.finish();
                game.stop();
            }
        }
    }

    if bench.enabled() {
        bench.finish();
    }

    Ok(())
}

#[cfg(test)]
mod architecture_audit;
pub mod assets;
pub mod bench;
pub mod collision;
pub mod display;
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
mod lighting_isolation;
#[cfg(test)]
mod lighting_leak_audit;
#[cfg(test)]
mod lighting_parity;
#[cfg(test)]
mod lighting_partition_audit;
#[cfg(test)]
mod lighting_vertical_audit;
pub mod loader;
pub mod logging;
pub mod materials;
pub mod perf;
pub mod props;
pub mod quality;
pub mod render;
pub mod settings;
pub mod spatial;
#[cfg(test)]
mod surface_audit;
#[cfg(test)]
mod test_support;
pub mod ui;

use std::cmp::Ordering;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glam::Vec3;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::video::{FullscreenType, Window};
use sdl2::{EventPump, Sdl, VideoSubsystem};

use bench::Bench;
use display::DisplayStatus;
use game::{AppState, Game};
use input::{InputHandler, MenuNavEvent, keycode_to_str};
use level::WalkableFloor;
use perf::PerfOverlay;
use render::{DrawableSize, Renderer, Vertex};
use settings::{Settings, WindowMode};
use ui::{SettingsAction, SettingsPage, UiGeometryCache, UiState, activate_settings_item};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often `LIMINAL_STATE_LOG` records the player state, in frames.
const STATE_LOG_INTERVAL: u64 = 5;

/// Items in the fixed main menu: Level Select, Settings, Exit.
const MAIN_MENU_ITEM_COUNT: usize = 3;

/// Items in the fixed pause menu: Resume, Settings, Return to Main Menu.
const PAUSE_MENU_ITEM_COUNT: usize = 3;

/// Items the level list appends after the installed levels: Load/Import, Back.
const LEVEL_SELECT_TRAILING_ITEMS: usize = 2;

/// Previous index in a `len`-item cyclic list, wrapping to the last item.
const fn menu_prev(index: usize, len: usize) -> usize {
    match index.checked_sub(1) {
        Some(previous) => previous,
        None => len.saturating_sub(1),
    }
}

/// Next index in a `len`-item cyclic list, wrapping to the first item.
const fn menu_next(index: usize, len: usize) -> usize {
    let next = index.saturating_add(1);
    if next < len { next } else { 0 }
}

/// Prints what the level's props and baked lighting cost, so hardware runs
/// (`PocketCHIP` over SSH) can be checked without a debugger: decoded models,
/// texture memory, draw calls, level-build time and the baked room baselines.
///
/// Developer telemetry: only printed when `LIMINAL_VERBOSE` is set.
// Startup CLI output that has no logger to route through.
#[allow(clippy::print_stdout)]
fn log_prop_usage(renderer: &Renderer) {
    if !logging::verbose() {
        return;
    }
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
        "[lighting] baked {} room(s) / {} baseline area(s) from {} fixture(s): {} wall + {} slab + {} prop blocker(s), baselines {:.2}..{:.2} (avg {:.2})",
        level.lighting.rooms,
        level.lighting.zones,
        level.lighting.lights,
        level.lighting.walls,
        level.lighting.blockers.saturating_sub(level.lighting.walls),
        level.lighting.props,
        level.lighting.min_baseline,
        level.lighting.max_baseline,
        level.lighting.average_baseline
    );
    println!(
        "[lightmaps] {} page(s), {} chart(s), {} chart texels, {} page texels ({} KiB), filled in {:.1} ms{}",
        level.lightmap_pages,
        level.lightmap_charts,
        level.lightmap_chart_texels,
        level.lightmap_texels,
        level.lightmap_texels.saturating_mul(3) / 1024,
        level.lightmap_millis,
        if level.lightmap_fallback {
            " (fallback: vertex lighting)"
        } else {
            ""
        }
    );
    println!(
        "[dynamic] {} object(s): {} draw call(s), {} vertices (the demonstration path; never part of the static bake)",
        renderer.dynamic_scene().len(),
        renderer.dynamic_scene().draw_count(),
        renderer.dynamic_scene().vertex_count()
    );
}

/// Parses `LIMINAL_SPAWN` overrides: `x,z,yaw_degrees` keeps the default eye
/// height above the local floor, `x,y,z,yaw_degrees` sets the eye explicitly.
/// Invalid input is ignored.
///
/// The three-number form returns `NAN` for Y, which the caller resolves against
/// the level's walkable floor (so the override works in elevated rooms too).
fn parse_spawn_override(value: &str) -> Option<[f32; 4]> {
    let parts: Vec<f32> = value
        .split(',')
        .map(|part| part.trim().parse::<f32>().ok())
        .collect::<Option<Vec<f32>>>()?;
    let numbers: Vec<f32> = parts
        .into_iter()
        .filter(|number| number.is_finite())
        .collect();
    match numbers.as_slice() {
        [x, z, yaw] => Some([*x, f32::NAN, *z, *yaw]),
        [x, y, z, yaw] => Some([*x, *y, *z, *yaw]),
        _ => None,
    }
}

/// Configures SDL OpenGL attributes before the window/context is created.
///
/// When `gles` is true, requests an OpenGL ES 2.0 context (`PocketCHIP` baseline);
/// otherwise the platform default profile is used so desktop development still
/// works. Double buffering is always requested.
///
/// The window's visual is *not* requested. On this driver GLX always returns a
/// 32-bit ARGB config even though the screen is 24-bit, which makes Xorg
/// composite every presented frame; no combination of `SDL_VIDEO_X11_VISUALID`,
/// `SDL_VIDEO_X11_WINDOW_VISUALID`, `SDL_VIDEO_X11_NODIRECTCOLOR`,
/// `SDL_GL_ALPHA_SIZE=0` or explicit RGB sizes changes it (a plain `glxgears`
/// gets the same visual). That is a property of the stack, and
/// `docs/presentation.md` measures what it costs and records that it cannot be
/// removed from inside the game.
fn configure_gl_attributes(video: &VideoSubsystem, gles: bool) {
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
/// once at startup when telemetry is enabled, and the interval in force is
/// returned for the caller.
fn apply_swap_interval(video: &VideoSubsystem, want_vsync: bool) -> i32 {
    let requested = if want_vsync {
        sdl2::video::SwapInterval::VSync
    } else {
        sdl2::video::SwapInterval::Immediate
    };
    match video.gl_set_swap_interval(requested) {
        Ok(()) => {
            let reported = video.gl_get_swap_interval();
            logging::info(format!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Ok, SDL_GL_GetSwapInterval -> {reported:?}",
            ));
            reported as i32
        }
        Err(error) => {
            let reported = video.gl_get_swap_interval();
            logging::info(format!(
                "[vsync] requested {requested:?}, SDL_GL_SetSwapInterval -> Err({error}), SDL_GL_GetSwapInterval -> {reported:?}",
            ));
            reported as i32
        }
    }
}

/// Root of the running installation.
///
/// The package root is the directory that owns `assets/`. It is found through
/// [`crate::assets::resolved_package_roots`], which searches `$LIMINAL_ASSET_ROOT`,
/// the executable's own directory (and its ancestors, covering both the flat
/// `Places/<executable>` layout and a macOS `.app` bundle's `Resources`), and
/// then the working directory. The compile-time crate path is a development-only
/// last resort, so a copied release build can never read the source tree it was
/// compiled from and report a false pass.
fn package_root() -> PathBuf {
    if let Some(assets) = crate::assets::resolve_asset_root()
        && let Some(root) = assets.parent()
    {
        // Canonicalise for display and for the working directory: a bundle's
        // `Contents/Resources` is reached through `Contents/MacOS/..`.
        return std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Points every relative asset path at the running installation.
///
/// The level loader, the prop catalogue, imported level packs and the settings
/// file are all resolved relative to the working directory, so an installed
/// package has to run from its own root. A development build already does this
/// and is left alone.
// Startup CLI output that has no logger to route through.
#[allow(clippy::print_stderr)]
fn use_package_assets() -> PathBuf {
    let package = package_root();
    let current = std::env::current_dir().ok();
    if current.as_deref() != Some(package.as_path())
        && let Err(error) = std::env::set_current_dir(&package)
    {
        eprintln!(
            "could not use the package directory {}: {error}",
            package.display()
        );
        eprintln!(
            "relative assets, levels and settings will resolve against {} instead",
            current
                .as_deref()
                .map_or_else(|| "<unknown>".to_string(), |dir| dir.display().to_string())
        );
    }
    package
}

/// Logs the resolved package directory and asset root at startup.
///
/// Developer telemetry: only printed when `LIMINAL_VERBOSE` is set.
fn log_package(package: &Path) {
    if !logging::verbose() {
        return;
    }
    logging::info(format!(
        "[package] {} (assets: {})",
        package.display(),
        package.join("assets/levels").display()
    ));
    match crate::assets::resolve_asset_root() {
        Some(assets) => {
            let shown = std::fs::canonicalize(&assets).unwrap_or(assets);
            logging::info(format!("[package] asset root: {}", shown.display()));
        }
        None => logging::info("[package] asset root: NONE"),
    }
}

/// The usable work area of one display, in logical pixels.
///
/// `(0, 0)` when the backend cannot report it, which the pure fitting rule
/// treats as "unknown" rather than "nothing fits".
fn usable_display_bounds(video: &VideoSubsystem, display_index: i32) -> (u32, u32) {
    video
        .display_usable_bounds(display_index)
        .map_or((0, 0), |rect| (rect.width(), rect.height()))
}

/// Creates the game window from the effective display settings.
///
/// The window opens at the saved windowed resolution ([`crate::settings::DEFAULT_WINDOW_WIDTH`]
/// x [`crate::settings::DEFAULT_WINDOW_HEIGHT`] on a fresh install), reduced to
/// fit the active display's work area when necessary, or as borderless
/// fullscreen when that is the selected mode. `allow_highdpi` keeps the drawable
/// at the Retina backing scale; the renderer reads the drawable, never the
/// logical size.
///
/// A size that had to be reduced to fit is adopted back into `settings` (and
/// therefore persisted and shown in Display), so the menu and the window never
/// disagree.
fn create_window(video: &VideoSubsystem, settings: &mut Settings) -> Result<Window, String> {
    // Configure the framebuffer/context attributes *before* the OpenGL window
    // is created. On Linux/EGL (PocketCHIP) the visual (depth buffer, double
    // buffering) is chosen at window creation, so setting these afterwards
    // would have no effect.
    configure_gl_attributes(video, true);

    let requested = settings.window_size();
    let usable = usable_display_bounds(video, 0);
    let (width, height) = display::fit_window_to_bounds(requested, usable);
    if (width, height) != requested {
        logging::info(format!(
            "[display] requested windowed {} does not fit the display work area ({}); \
             opening {} instead",
            display::format_resolution(requested),
            display::format_resolution(usable),
            display::format_resolution((width, height))
        ));
    }

    let build_window = || {
        let mut builder = video.window("Places", width, height);
        builder
            .position_centered()
            .resizable()
            .allow_highdpi()
            .opengl();
        if settings.window_mode() == WindowMode::Fullscreen {
            builder.fullscreen_desktop();
        }
        builder.build()
    };
    let window = if let Ok(window) = build_window() {
        window
    } else {
        configure_gl_attributes(video, false);
        build_window().map_err(|e| format!("Failed to create window: {e}"))?
    };

    let actual = window.size();
    if settings.window_mode() == WindowMode::Windowed
        && actual.0 > 0
        && actual.1 > 0
        && actual != settings.window_size()
    {
        settings.adopt_window_size(actual.0, actual.1);
    }
    Ok(window)
}

/// Creates the SDL video subsystem and the game window.
fn create_sdl_and_window(settings: &mut Settings) -> Result<(Sdl, VideoSubsystem, Window), String> {
    let sdl_context = sdl2::init().map_err(|e| format!("Failed to init SDL2: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    // SDL enables Unicode text input (and therefore the platform IME) implicitly
    // as soon as the video subsystem starts. This game only consumes raw key
    // events, never composed text, so disable it: on macOS this is the code path
    // that engages InputMethodKit (`interpretKeyEvents` on SDL's text responder).
    video_subsystem.text_input().stop();

    let window = create_window(&video_subsystem, settings)?;
    Ok((sdl_context, video_subsystem, window))
}

/// Logs one startup line for the asset catalog: how many logical assets it
/// declares and which environment themes organize them. Lookup itself is
/// resolved once per level load, never per frame.
///
/// Developer telemetry: only printed when `LIMINAL_VERBOSE` is set.
fn log_asset_catalog(level_manager: &loader::LevelManager) {
    if !logging::verbose() {
        return;
    }
    let catalog = level_manager.prop_catalog();
    let themes: Vec<&str> = catalog
        .themes()
        .iter()
        .map(|theme| theme.id.as_str())
        .collect();
    let themes = if themes.is_empty() {
        "(none)".to_string()
    } else {
        themes.join(", ")
    };
    logging::info(format!(
        "[assets] {} placeable asset(s) of {} catalog entries; themes: {themes}",
        catalog.len(),
        catalog.assets().len()
    ));
}

/// Spawns the dynamic demonstration for levels that ship with one.
///
/// The engine's dynamic path is generic (`Renderer::set_dynamic_demo`), but the
/// demonstration is content: it lives in Places Demo so the engine regression
/// fixtures and their benchmarks are unaffected by it.
fn spawn_level_demonstration(renderer: &mut Renderer, loaded: &loader::LoadedLevel) {
    if loaded.level.id == loader::DEMO_LEVEL_ID {
        renderer.set_dynamic_demo(&loaded.level);
    }
}

/// Builds the renderer for the initial level and applies the persisted texture
/// filtering plus the three benchmark submission switches.
///
/// The switches keep the level build, the batching, the draw order and the
/// shader identical and change exactly one submission decision each, which is
/// how a single build measures what culling, indexing and vertex packing are
/// each worth on real hardware.
fn create_renderer(
    window: &Window,
    video_subsystem: &VideoSubsystem,
    level: &loader::LoadedLevel,
    settings: &Settings,
    bench: &Bench,
) -> Result<Renderer, String> {
    let mut renderer = Renderer::new(window, video_subsystem)
        .map_err(|e| format!("Failed to initialize renderer: {e}"))?;
    // The quality profile decides how large a texture may reach the GPU, and
    // the lightmap mode is a build-time choice, so both are applied before the
    // first level upload rather than after it.
    renderer.set_quality(settings.quality_profile());
    renderer.set_lightmaps_requested(settings.lightmaps_enabled());
    renderer.set_level(level);
    spawn_level_demonstration(&mut renderer, level);
    renderer.set_texture_filtering(&settings.texture_filtering);
    renderer.set_culling(!bench.no_cull());
    renderer.set_indexing(!bench.no_index());
    Ok(renderer)
}

/// Applies the effective `VSync` setting *after* the GL context exists and is
/// current.
///
/// `SDL_GL_SetSwapInterval` fails outright without a current context, which is
/// why the request used to be dropped and the renderer's own unconditional
/// VSync-on call won instead. The same helper is called at runtime whenever the
/// player changes the setting, so `VSync` applies immediately instead of at the
/// next launch.
fn configure_vsync(video_subsystem: &VideoSubsystem, bench: &mut Bench, settings: &Settings) {
    let swap_interval = apply_swap_interval(video_subsystem, settings.vsync_enabled());
    bench.set_reported_swap_interval(swap_interval);
}

/// Creates the player state, the camera and the spawn from a level's spawn
/// point.
fn new_game(level: &loader::LoadedLevel) -> (Game, Vec3, f32) {
    let spawn_pos = game::spawn_position(&level.level);
    let spawn_yaw = level.level.spawn.yaw_degrees.to_radians();
    let game = Game::new(
        spawn_pos,
        spawn_yaw,
        level.level.collision_aabbs(),
        WalkableFloor::from_level(&level.level),
    );
    (game, spawn_pos, spawn_yaw)
}

/// Display names of the installed levels, in discovery order.
fn level_entry_names(level_manager: &loader::LevelManager) -> Vec<String> {
    level_manager
        .entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

/// UI state seeded with the installed level names.
fn new_ui_state(level_manager: &loader::LevelManager) -> UiState {
    UiState {
        level_entries: level_entry_names(level_manager),
        ..UiState::new()
    }
}

/// The one-shot framebuffer capture path from `LIMINAL_CAPTURE`, if set.
fn capture_path_from_env() -> Option<PathBuf> {
    std::env::var("LIMINAL_CAPTURE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The frame `LIMINAL_CAPTURE` should be taken on, 1-based.
///
/// The default is the first rendered frame, which is what every existing
/// capture does. `LIMINAL_CAPTURE_FRAME=n` waits for frame `n` first, so a
/// capture can show something that changes over time (the washer-drum
/// demonstration turns as the frame loop runs) without a second launch.
fn capture_frame_from_env() -> u64 {
    std::env::var("LIMINAL_CAPTURE_FRAME")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|frame| *frame > 0)
        .unwrap_or(1)
}

/// The parsed `LIMINAL_SPAWN` override, if set.
fn spawn_override_from_env() -> Option<[f32; 4]> {
    std::env::var("LIMINAL_SPAWN")
        .ok()
        .and_then(|value| parse_spawn_override(&value))
}

/// Opens the `LIMINAL_STATE_LOG` CSV file, reporting why it cannot be used.
// Startup CLI output that has no logger to route through.
#[allow(clippy::print_stderr)]
fn open_state_log() -> Option<std::fs::File> {
    let path = std::env::var("LIMINAL_STATE_LOG")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| eprintln!("LIMINAL_STATE_LOG: cannot open {path}: {error}"))
        .ok()
}

/// Applies a `LIMINAL_SPAWN` override.
///
/// A three-number override keeps the standard eye height above the local floor;
/// a four-number one sets the eye explicitly for a shot.
fn apply_spawn_override(
    game: &mut Game,
    spawn_pos: &mut Vec3,
    spawn_yaw: &mut f32,
    spawn: [f32; 4],
) {
    let [x, y, z, yaw] = spawn;
    let eye_y = if y.is_finite() {
        y
    } else {
        game::spawn_eye_y(&game.floor, x, z)
    };
    *spawn_pos = Vec3::new(x, eye_y, z);
    *spawn_yaw = yaw.to_radians();
    game.reset_level(
        *spawn_pos,
        *spawn_yaw,
        game.walls.clone(),
        game.floor.clone(),
    );
}

/// Boots straight into the level named by `LIMINAL_LEVEL`.
///
/// This is how the shipped demo and the bench levels are checked on the
/// `PocketCHIP`, where the menu cannot be driven over SSH:
/// `LIMINAL_LEVEL=places_demo ./liminal-rust`.
///
/// When the request names the level that is already loaded (the ordinary
/// `LIMINAL_LEVEL=places_demo` case), the renderer is left alone: rebuilding the
/// same level would repeat the whole cold level build and lightmap bake.
// Developer CLI output that has no logger to route through.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn apply_level_request(
    level_manager: &loader::LevelManager,
    renderer: &mut Renderer,
    game: &mut Game,
    bench: &Bench,
    spawn_pos: &mut Vec3,
    spawn_yaw: &mut f32,
    current_level: &mut Option<loader::LoadedLevel>,
) {
    let Some(requested) = std::env::var("LIMINAL_LEVEL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let loaded_id = current_level
        .as_ref()
        .map_or("", |level| level.entry.id.as_str());
    let index = level_manager
        .entries()
        .iter()
        .position(|entry| entry.id == requested || entry.name.eq_ignore_ascii_case(&requested));
    match index.and_then(|index| level_manager.get_entry(index).cloned()) {
        Some(entry) if entry.id == loaded_id => {
            println!(
                "LIMINAL_LEVEL: '{}' ({}) is already the loaded level",
                entry.name, entry.id
            );
            game.set_app_state(AppState::Playing);
            if pause_requested() {
                game.set_app_state(AppState::Paused);
            }
        }
        Some(entry) => match level_manager.load_level(&entry) {
            Ok(loaded) => {
                println!(
                    "LIMINAL_LEVEL: loading '{}' ({}) - {} props",
                    loaded.level.name,
                    loaded.level.id,
                    loaded.level.props.len()
                );
                renderer.set_level(&loaded);
                spawn_level_demonstration(renderer, &loaded);
                renderer.set_culling(!bench.no_cull());
                renderer.set_indexing(!bench.no_index());
                log_prop_usage(renderer);
                *spawn_pos = game::spawn_position(&loaded.level);
                *spawn_yaw = loaded.level.spawn.yaw_degrees.to_radians();
                game.reset_level(
                    *spawn_pos,
                    *spawn_yaw,
                    loaded.level.collision_aabbs(),
                    WalkableFloor::from_level(&loaded.level),
                );
                *current_level = Some(loaded);
                game.set_app_state(AppState::Playing);
                // `LIMINAL_PAUSE=1` opens the pause menu on the first frame so
                // the pause UI can be captured and compared without a keyboard.
                // It changes nothing about how the menu draws.
                if pause_requested() {
                    game.set_app_state(AppState::Paused);
                }
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

/// True when `LIMINAL_PAUSE` asks for the pause menu on the first frame.
fn pause_requested() -> bool {
    std::env::var("LIMINAL_PAUSE").is_ok_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off"
        )
    })
}

/// Renders one frame of the running level to `path` as a PNG.
///
/// This is the `LIMINAL_CAPTURE=frame.png` developer / hardware capture: how
/// prop rendering is inspected on the `PocketCHIP` over SSH (or on a desktop
/// where the window cannot be screenshotted).
// CLI output that has no logger to route through.
#[allow(clippy::print_stdout, clippy::print_stderr)]
fn write_capture(renderer: &Renderer, path: &Path) {
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
}

/// Every piece of mutable state the frame loop touches.
///
/// Grouping them keeps `main` a readable setup sequence and lets each step of a
/// frame be reviewed (and unit-tested) on its own.
struct FrameLoop<'a> {
    window: &'a mut Window,
    video_subsystem: &'a VideoSubsystem,
    event_pump: &'a mut EventPump,
    renderer: &'a mut Renderer,
    level_manager: &'a mut loader::LevelManager,
    game: &'a mut Game,
    bench: &'a mut Bench,
    input_handler: &'a mut InputHandler,
    settings: &'a mut Settings,
    ui_state: &'a mut UiState,
    perf_overlay: &'a mut PerfOverlay,
    ui_cache: &'a mut UiGeometryCache,
    ui_scratch: &'a mut Vec<Vertex>,
    applied_filtering: &'a mut String,
    capture_path: &'a mut Option<PathBuf>,
    capture_at_frame: u64,
    state_log: &'a mut Option<std::fs::File>,
    spawn_pos: &'a mut Vec3,
    spawn_yaw: &'a mut f32,
    /// The level definition currently uploaded to the renderer, kept so a live
    /// quality or lightmap change can rebuild its GPU resources without
    /// re-reading the file and without touching the game's player state.
    current_level: &'a mut Option<loader::LoadedLevel>,
    /// What the windowing backend reports: actual window state, used by the
    /// Display screen and refreshed once per frame.
    display_status: &'a mut DisplayStatus,
    /// True on the frame a windowed size was just applied, so the size read
    /// back from the backend is not mistaken for a player resize.
    window_apply_in_flight: bool,
    /// Last `(window_w, window_h, drawable_w, drawable_h)` pair logged, so a
    /// HiDPI/backing-scale change is reported exactly once.
    last_logged_window: (u32, u32, u32, u32),
}

impl FrameLoop<'_> {
    /// Runs frames until the game stops.
    fn run(&mut self) {
        while self.game.is_running() {
            self.frame();
        }
    }

    /// One complete frame: input, simulation, render, present, telemetry.
    fn frame(&mut self) {
        // Frame boundary for the benchmark harness: everything from here to the
        // end of `gl_swap_window` is one complete frame, swap included.
        let frame_begin = Instant::now();
        self.game.update_timing();
        self.perf_overlay.update(self.game.delta_seconds());

        self.pump_events();

        if self.input_handler.quit_requested() {
            self.game.stop();
        }

        // Apply whatever the player changed in Settings before this frame
        // simulates or draws, then refresh what the windowing backend actually
        // did (a window change, a monitor move, a HiDPI backing-scale change).
        self.apply_pending_settings();
        self.refresh_display_status();

        // Update player movement (only active during AppState::Playing)
        self.game
            .update_player_movement(self.input_handler.state(), self.settings);
        self.log_player_state();
        // Advance the dynamic objects (the demonstration drum and any other
        // spawned object) once per frame: transform only, never a geometry or
        // lightmap rebuild.
        self.renderer.update_dynamic(self.game.delta_seconds());
        let frame_update_done = Instant::now();

        self.render_and_present(frame_begin, frame_update_done);
    }

    /// Applies the subsystem updates a settings change owes.
    ///
    /// This is the only place the menu's mutations reach the renderer or the
    /// window: `activate_settings_item` updates the authoritative [`Settings`]
    /// and records what that implies, and this performs the minimum work for
    /// each affected system. Nothing here reloads a level or resets the game:
    /// a graphics rebuild re-uploads GPU resources from the level definition
    /// already resident, leaving the player, camera and pause state untouched.
    fn apply_pending_settings(&mut self) {
        let apply = self.settings.take_pending_apply();
        if !apply.any() {
            return;
        }
        if apply.window
            && let Err(error) = self.apply_window_settings()
        {
            self.ui_state
                .set_status(format!("Could not change the window: {error}"), true);
        }
        if apply.vsync {
            let interval = apply_swap_interval(self.video_subsystem, self.settings.vsync_enabled());
            self.bench.set_reported_swap_interval(interval);
        }
        if apply.graphics_rebuild {
            self.rebuild_graphics_resources();
        }
    }

    /// Applies the window mode and size to the live window.
    ///
    /// A windowed size that does not fit the work area is reduced to fit and
    /// adopted back into the settings, so restoring from fullscreen always
    /// lands on a usable window and the menu shows the real one.
    fn apply_window_settings(&mut self) -> Result<(), String> {
        if self.settings.window_mode() == WindowMode::Fullscreen {
            return self.window.set_fullscreen(FullscreenType::Desktop);
        }
        self.window.set_fullscreen(FullscreenType::Off)?;
        let requested = self.settings.window_size();
        let fitted = display::fit_window_to_bounds(requested, self.display_status.usable_bounds);
        self.window
            .set_size(fitted.0, fitted.1)
            .map_err(|error| error.to_string())?;
        if fitted != requested {
            self.settings.adopt_window_size(fitted.0, fitted.1);
        }
        // The backend may report the pre-apply size for a frame; do not mistake
        // that for a player resize (see `refresh_display_status`).
        self.window_apply_in_flight = true;
        Ok(())
    }

    /// Rebuilds the renderer resources the quality profile and the lightmap
    /// mode decide, from the level already resident.
    ///
    /// The profile changes how large a texture may reach the GPU and how the
    /// lightmap atlas is baked, so the GPU-side texture caches are released and
    /// the current level is re-uploaded at the new profile. The game world —
    /// player position, camera, pause state, the `Game` struct — is not touched:
    /// this is a renderer rebuild, not a level load.
    fn rebuild_graphics_resources(&mut self) {
        self.renderer.set_quality(self.settings.quality_profile());
        self.renderer
            .set_lightmaps_requested(self.settings.lightmaps_enabled());
        let Some(level) = self.current_level.as_ref() else {
            return;
        };
        self.renderer.release_profile_textures();
        self.renderer.set_level(level);
        crate::logging::info(format!(
            "[settings] rebuilt GPU resources at quality '{}' (lightmaps {})",
            self.settings.quality_profile().name(),
            if self.settings.lightmaps_enabled() {
                "on"
            } else {
                "off"
            }
        ));
    }

    /// Refreshes the actual window/display state the Display screen reports.
    ///
    /// The backend is the source of truth: a fullscreen request the platform
    /// refused, a manual window resize and a monitor or backing-scale change all
    /// show up here (and a manual resize is adopted into the settings, so the
    /// menu and the persisted configuration follow the real window).
    fn refresh_display_status(&mut self) {
        let mode = match self.window.fullscreen_state() {
            FullscreenType::Off => WindowMode::Windowed,
            FullscreenType::True | FullscreenType::Desktop => WindowMode::Fullscreen,
        };
        let window_size = self.window.size();
        if mode == WindowMode::Windowed
            && !self.window_apply_in_flight
            && window_size.0 > 0
            && window_size.1 > 0
            && window_size != self.settings.window_size()
        {
            self.settings
                .adopt_window_size(window_size.0, window_size.1);
        }
        self.window_apply_in_flight = false;
        let display_index = self.window.display_index().unwrap_or(0);
        let usable_bounds = usable_display_bounds(self.video_subsystem, display_index);
        let desktop_size = self
            .video_subsystem
            .display_bounds(display_index)
            .map_or((0, 0), |rect| (rect.width(), rect.height()));
        self.display_status.mode = mode;
        self.display_status.window_size = window_size;
        self.display_status.usable_bounds = if usable_bounds == (0, 0) {
            self.display_status.usable_bounds
        } else {
            usable_bounds
        };
        self.display_status.desktop_size = desktop_size;

        // Report the logical and drawable sizes (and their ratio) once per
        // change when telemetry is on: this is the line that proves the
        // renderer is using Retina pixels rather than the logical window size.
        let (drawable_width, drawable_height) = self.window.drawable_size();
        let logged = (
            window_size.0,
            window_size.1,
            drawable_width,
            drawable_height,
        );
        if logged != self.last_logged_window {
            self.last_logged_window = logged;
            logging::info(format!(
                "[window] logical {}x{} | drawable {}x{} pixels | backing scale {:.1}x{}",
                window_size.0,
                window_size.1,
                drawable_width,
                drawable_height,
                if window_size.0 == 0 {
                    0.0
                } else {
                    f64::from(drawable_width) / f64::from(window_size.0)
                },
                if mode == WindowMode::Fullscreen {
                    " | fullscreen"
                } else {
                    ""
                }
            ));
        }
    }

    /// `LIMINAL_STATE_LOG=file.csv`: append the player state every few frames.
    ///
    /// Development diagnostics for control validation on a real machine; it
    /// never changes gameplay and is inert unless the variable is set.
    fn log_player_state(&mut self) {
        if let Some(log) = self.state_log.as_mut()
            && self.game.frame_count().is_multiple_of(STATE_LOG_INTERVAL)
        {
            let _ = writeln!(
                log,
                "{},{:.4},{:.4},{:.4},{:.4},{:.4}",
                self.game.frame_count(),
                self.game.player_position.x,
                self.game.player_position.y,
                self.game.player_position.z,
                self.game.player_yaw,
                self.game.player_pitch
            );
        }
    }

    /// Pumps the SDL event queue for this frame.
    fn pump_events(&mut self) {
        // The iterator borrows the pump for as long as it lives, so it is
        // re-created per event; that keeps the borrow short enough for
        // `handle_event` to borrow `self` in between.
        loop {
            let next = self.event_pump.poll_iter().next();
            let Some(event) = next else { break };
            if !self.handle_event(&event) {
                break;
            }
        }
    }

    /// Handles one SDL event; returns `false` when the pump should stop.
    fn handle_event(&mut self, event: &Event) -> bool {
        if let Event::Quit { .. } = event {
            self.game.stop();
            return false;
        }

        // 1. If currently waiting for key rebinding in Settings
        if let Some(action) = self.ui_state.rebinding_action {
            if let Event::KeyDown {
                keycode: Some(key), ..
            } = event
            {
                if *key == Keycode::Escape {
                    self.ui_state.cancel_rebinding();
                } else {
                    let key_str = keycode_to_str(*key);
                    match self.settings.bindings.set_key(action, &key_str) {
                        Ok(()) => {
                            let label = crate::settings::action_label(action);
                            self.ui_state
                                .set_status(format!("Bound {label} to [{key_str}]"), false);
                            if let Err(error) = self.settings.save() {
                                self.ui_state
                                    .set_status(format!("Could not save settings: {error}"), true);
                            }
                        }
                        Err(err) => {
                            self.ui_state.set_status(err, true);
                        }
                    }
                    self.ui_state.rebinding_action = None;
                }
            }
            return true;
        }

        // Performance overlay toggle with '-' key (hidden by default, reserved
        // so it can never also be a gameplay binding)
        if let Event::KeyDown {
            keycode: Some(Keycode::Minus | Keycode::KpMinus),
            repeat: false,
            ..
        } = event
        {
            self.perf_overlay.toggle();
            return true;
        }

        // 2. Menu navigation for non-playing states (independent of gameplay bindings)
        if self.game.app_state() == AppState::Playing {
            // 3. Gameplay active (AppState::Playing)
            if let Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                ..
            } = event
            {
                self.input_handler.clear_gameplay_inputs();
                self.game.handle_escape(); // Opens Pause menu
            } else {
                self.input_handler
                    .handle_gameplay_event(event, &self.settings.bindings);
            }
        } else if let Some(nav) = InputHandler::poll_menu_nav_event(event) {
            self.handle_menu_nav(nav);
        }
        true
    }

    /// Dispatches one menu navigation event for the current screen.
    fn handle_menu_nav(&mut self, nav: MenuNavEvent) {
        match nav {
            MenuNavEvent::Up => self.cycle_selection(true),
            MenuNavEvent::Down => self.cycle_selection(false),
            MenuNavEvent::Left => self.adjust_settings(-1),
            MenuNavEvent::Right => self.adjust_settings(1),
            MenuNavEvent::Activate => self.activate(),
            MenuNavEvent::Back => {
                let in_settings = matches!(
                    self.game.app_state(),
                    AppState::Settings | AppState::PauseSettings
                );
                if in_settings && !self.settings_back() {
                    // A sub-page stepped back to the Settings root.
                } else {
                    self.game.handle_escape();
                }
            }
        }
    }

    /// Moves the current menu's selection, wrapping at both ends.
    const fn cycle_selection(&mut self, up: bool) {
        let level_count = self
            .ui_state
            .level_entries
            .len()
            .saturating_add(LEVEL_SELECT_TRAILING_ITEMS);
        match (self.game.app_state(), up) {
            (AppState::MainMenu, true) => {
                self.ui_state.main_menu_idx =
                    menu_prev(self.ui_state.main_menu_idx, MAIN_MENU_ITEM_COUNT);
            }
            (AppState::MainMenu, false) => {
                self.ui_state.main_menu_idx =
                    menu_next(self.ui_state.main_menu_idx, MAIN_MENU_ITEM_COUNT);
            }
            (AppState::LevelSelect, true) => {
                self.ui_state.level_select_idx =
                    menu_prev(self.ui_state.level_select_idx, level_count);
            }
            (AppState::LevelSelect, false) => {
                self.ui_state.level_select_idx =
                    menu_next(self.ui_state.level_select_idx, level_count);
            }
            (AppState::Paused, true) => {
                self.ui_state.pause_menu_idx =
                    menu_prev(self.ui_state.pause_menu_idx, PAUSE_MENU_ITEM_COUNT);
            }
            (AppState::Paused, false) => {
                self.ui_state.pause_menu_idx =
                    menu_next(self.ui_state.pause_menu_idx, PAUSE_MENU_ITEM_COUNT);
            }
            (AppState::Settings | AppState::PauseSettings, true) => {
                let count = self.ui_state.settings_page.item_count();
                self.ui_state.settings_idx = menu_prev(self.ui_state.settings_idx, count);
            }
            (AppState::Settings | AppState::PauseSettings, false) => {
                let count = self.ui_state.settings_page.item_count();
                self.ui_state.settings_idx = menu_next(self.ui_state.settings_idx, count);
            }
            (AppState::Playing, _) => {}
        }
    }

    /// Left/right on the settings screen adjusts or rebinds the selected item.
    ///
    /// A section or Back row ignores left/right (Enter opens or leaves); every
    /// other row is a value selector, a toggle or a rebind.
    fn adjust_settings(&mut self, direction: i32) {
        if matches!(
            self.game.app_state(),
            AppState::Settings | AppState::PauseSettings
        ) {
            let page = self.ui_state.settings_page;
            let index = self.ui_state.settings_idx;
            let _ = activate_settings_item(
                page,
                index,
                self.ui_state,
                self.settings,
                self.display_status,
                direction,
            );
            self.save_settings();
        }
    }

    /// Persists settings after a change, surfacing a failure to the player.
    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            self.ui_state
                .set_status(format!("Could not save settings: {error}"), true);
        }
    }

    /// Backs out of Settings one level.
    ///
    /// A sub-page (Graphics/Display/Controls) returns to the Settings root and
    /// returns `false`; the root returns `true`, which means "leave the
    /// screen". This is what makes Escape walk the section tree before it
    /// resumes the game.
    fn settings_back(&mut self) -> bool {
        if self.ui_state.settings_page == SettingsPage::Root {
            true
        } else {
            self.ui_state.settings_page = SettingsPage::Root;
            self.ui_state.settings_idx = 0;
            self.ui_state.clear_status();
            false
        }
    }

    /// Activates the selected item on the current menu.
    fn activate(&mut self) {
        match self.game.app_state() {
            AppState::MainMenu => self.activate_main_menu(),
            AppState::LevelSelect => self.activate_level_select(),
            AppState::Paused => self.activate_pause_menu(),
            AppState::Settings | AppState::PauseSettings => self.activate_settings(),
            AppState::Playing => {}
        }
    }

    /// Activates one Settings row: opens a section, leaves the screen or
    /// adjusts/rebinds the selected option.
    fn activate_settings(&mut self) {
        let page = self.ui_state.settings_page;
        let index = self.ui_state.settings_idx;
        let action = activate_settings_item(
            page,
            index,
            self.ui_state,
            self.settings,
            self.display_status,
            1,
        );
        self.save_settings();
        match action {
            SettingsAction::Open(next) => {
                self.ui_state.settings_page = next;
                self.ui_state.settings_idx = 0;
                self.ui_state.clear_status();
            }
            SettingsAction::Back => {
                if self.settings_back() {
                    let target = if self.game.app_state() == AppState::PauseSettings {
                        AppState::Paused
                    } else {
                        AppState::MainMenu
                    };
                    self.goto(target);
                }
            }
            SettingsAction::None => {}
        }
    }

    /// Switches screens, dropping any status line that belonged to the old one.
    ///
    /// Without this, "FOV updated" from Settings would appear on the level list
    /// and "Load failed: ..." would appear in Settings, because one `UiState`
    /// field backs every screen's status line.
    fn goto(&mut self, state: AppState) {
        if self.game.app_state() != state {
            self.ui_state.clear_status();
        }
        self.game.set_app_state(state);
    }

    /// Main menu: open the level list, open settings, or quit.
    fn activate_main_menu(&mut self) {
        match self.ui_state.main_menu_idx {
            0 => {
                // Discovery metadata is cached at startup and refreshed after
                // imports; opening the level list must not re-scan/re-extract
                // packs.
                self.refresh_level_entries();
                self.goto(AppState::LevelSelect);
            }
            1 => {
                self.ui_state.settings_page = SettingsPage::Root;
                self.ui_state.settings_idx = 0;
                self.goto(AppState::Settings);
            }
            2 => self.game.stop(),
            _ => {}
        }
    }

    /// Refreshes the cached level names from the manager.
    fn refresh_level_entries(&mut self) {
        self.ui_state.level_entries = level_entry_names(self.level_manager);
    }

    /// Level list: load the selected level, import packs, or go back.
    fn activate_level_select(&mut self) {
        let num_levels = self.level_manager.entries().len();
        let selected = self.ui_state.level_select_idx;
        match selected.cmp(&num_levels) {
            // One of the installed levels: load it.
            Ordering::Less => self.load_level_at(selected),
            // `Import Levels`.
            Ordering::Equal => self.import_levels(),
            // `Back`.
            Ordering::Greater => self.goto(AppState::MainMenu),
        }
    }

    /// Loads the level at `index` and drops the player into it.
    fn load_level_at(&mut self, index: usize) {
        let Some(entry) = self.level_manager.get_entry(index) else {
            return;
        };
        match self.level_manager.load_level(entry) {
            Ok(loaded) => {
                self.renderer.set_level(&loaded);
                spawn_level_demonstration(self.renderer, &loaded);
                *self.spawn_pos = game::spawn_position(&loaded.level);
                *self.spawn_yaw = loaded.level.spawn.yaw_degrees.to_radians();
                self.game.reset_level(
                    *self.spawn_pos,
                    *self.spawn_yaw,
                    loaded.level.collision_aabbs(),
                    WalkableFloor::from_level(&loaded.level),
                );
                // Keep the definition resident: a live quality or lightmap
                // change rebuilds this level's GPU resources from it.
                *self.current_level = Some(loaded);
                self.input_handler.clear_gameplay_inputs();
                self.ui_state.clear_status();
                self.game.set_app_state(AppState::Playing);
            }
            Err(err) => {
                crate::logging::warn(format!("[levels] {} failed to load: {err}", entry.name));
                self.ui_state
                    .set_status(format!("Could not load {}: {err}", entry.name), true);
            }
        }
    }

    /// Imports packs waiting in the import folders and refreshes the level list.
    fn import_levels(&mut self) {
        match self.level_manager.import_available() {
            Ok(count) => {
                // `import_available` rescans after importing.
                self.refresh_level_entries();
                if count > 0 {
                    self.ui_state.set_status(
                        format!("Imported {count} level(s). Select one from the list."),
                        false,
                    );
                } else {
                    self.ui_state
                        .set_status("No new levels found (put .json or .zip in import/)", false);
                }
            }
            Err(err) => {
                self.ui_state
                    .set_status(format!("Import failed: {err}"), true);
            }
        }
    }

    /// Pause menu: resume, open settings, or return to the main menu.
    fn activate_pause_menu(&mut self) {
        match self.ui_state.pause_menu_idx {
            0 => {
                self.input_handler.clear_gameplay_inputs();
                self.goto(AppState::Playing);
            }
            1 => {
                self.ui_state.settings_page = SettingsPage::Root;
                self.ui_state.settings_idx = 0;
                self.goto(AppState::PauseSettings);
            }
            2 => self.goto(AppState::MainMenu),
            _ => {}
        }
    }

    /// Draws the frame, handles the one-shot capture, presents and records it.
    fn render_and_present(&mut self, frame_begin: Instant, frame_update_done: Instant) {
        // Use the physical drawable size, not the logical window size, so HiDPI
        // (Retina) backing scale and monitor changes are handled automatically.
        let (drawable_width, drawable_height) = self.window.drawable_size();
        let drawable = DrawableSize::new(drawable_width, drawable_height);

        // Minimized/hidden windows report a zero-sized drawable. Skip rendering to
        // avoid invalid GL state and keep timing fresh so restoring does not jump.
        if drawable.is_empty() {
            self.game.reset_timing();
            std::thread::sleep(std::time::Duration::from_millis(16));
            return;
        }

        self.renderer.set_drawable_size(drawable);

        // Render scene
        let (cam_pos, cam_yaw, cam_pitch) = match self.game.app_state() {
            AppState::Playing | AppState::Paused | AppState::PauseSettings => (
                self.game.player_position,
                self.game.player_yaw,
                self.game.player_pitch,
            ),
            AppState::MainMenu | AppState::LevelSelect | AppState::Settings => {
                // Static menu background camera
                (*self.spawn_pos, *self.spawn_yaw, 0.0)
            }
        };
        // `LIMINAL_CAMERA=yaw[,pitch]` pins the camera so a hardware benchmark
        // measures the same view twice; it never changes gameplay.
        let (cam_yaw, cam_pitch) = match self.bench.camera_override() {
            Some((yaw, pitch)) => (yaw.to_radians(), pitch.to_radians()),
            None => (cam_yaw, cam_pitch),
        };

        let skip_render = self.bench.skip_render();
        if !skip_render {
            self.renderer
                .render_scene(cam_pos, cam_yaw, cam_pitch, self.settings.fov_degrees);
        }
        let frame_render_done = Instant::now();
        // Apply a changed texture filtering setting to existing GL textures
        // without re-uploading their pixel data.
        if self.settings.texture_filtering != *self.applied_filtering {
            self.renderer
                .set_texture_filtering(&self.settings.texture_filtering);
            self.applied_filtering
                .clone_from(&self.settings.texture_filtering);
        }

        // Menu/settings UI geometry is cached and only rebuilt when its inputs
        // change. The (debug) performance overlay is appended on demand.
        let ui_vertices = self.ui_cache.get(
            self.game.app_state(),
            self.ui_state,
            self.settings,
            self.display_status,
            APP_VERSION,
        );
        if skip_render {
            // `LIMINAL_BENCH_NORENDER=1`: measure the presentation path alone.
        } else if self.perf_overlay.is_visible() {
            self.ui_scratch.clear();
            self.ui_scratch.extend_from_slice(ui_vertices);
            self.ui_scratch
                .extend_from_slice(self.perf_overlay.cached_vertices());
            self.renderer.render_ui(self.ui_scratch);
        } else {
            self.renderer.render_ui(ui_vertices);
        }
        // `LIMINAL_BENCH_FINISH=1`: force the GL pipeline to drain before the
        // swap timing point, so `render_ms` is renderer completion time rather
        // than "how much of the frame the driver happened to absorb".
        if self.bench.finish_before_swap() {
            self.renderer.finish();
        }
        let frame_ui_done = Instant::now();

        if self.game.frame_count() >= self.capture_at_frame
            && let Some(path) = self.capture_path.take()
        {
            write_capture(self.renderer, &path);
            self.game.stop();
        }

        // Swap window buffer (double buffered, VSync synchronized)
        if !self.bench.skip_swap() {
            self.window.gl_swap_window();
        }
        let frame_swap_done = Instant::now();

        if self.bench.enabled() {
            self.bench.record_frame(
                frame_begin,
                frame_update_done,
                frame_render_done,
                frame_ui_done,
                frame_swap_done,
                self.renderer.render_stats(),
            );
            if self.bench.is_complete() {
                self.bench.finish();
                self.game.stop();
            }
        }
    }
}

/// Logs the effective runtime settings at startup when telemetry is enabled.
///
/// This is the line that makes a startup override visible: it reports what the
/// process is really running with, not what the saved file says.
fn log_effective_settings(settings: &Settings) {
    if !logging::verbose() {
        return;
    }
    logging::info(format!(
        "[settings] quality {} (saved {}){} | lightmaps {} | vsync {} | filtering {} | window {} {}",
        settings.quality_profile().name(),
        settings.quality,
        if settings.quality_overridden() {
            " [startup override]"
        } else {
            ""
        },
        on_off(settings.lightmaps_enabled()),
        on_off(settings.vsync_enabled()),
        settings.texture_filtering,
        settings.window_mode().label(),
        display::format_resolution(settings.window_size()),
    ));
}

const fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// Boots SDL, points relative paths at the running installation, loads the
/// persisted settings (with any startup overrides) and creates the window.
fn bootstrap() -> Result<(Sdl, VideoSubsystem, Window, Settings, Bench), String> {
    let package = use_package_assets();
    log_package(&package);
    // X11 process identity, required for the App Center launcher and window
    // managers to associate the window with this app. Kept as the historical
    // `io.vitrallis.liminalrust` package id on purpose: it is a launcher/session
    // key, not a display name, and changing it would orphan existing installs.
    sdl2::hint::set("SDL_VIDEO_X11_WMCLASS", "io.vitrallis.liminalrust");
    sdl2::hint::set("SDL_APP_NAME", "Places");

    // The benchmark harness is parsed first because `LIMINAL_VSYNC` is its
    // switch and has to be folded into the settings before the swap interval is
    // configured.
    let bench = Bench::new();
    let mut settings = Settings::load_or_default();
    settings.apply_startup_overrides(bench.vsync_override());
    settings.ensure_saved();
    log_effective_settings(&settings);

    let (sdl_context, video_subsystem, window) = create_sdl_and_window(&mut settings)?;
    // Boot applies every setting explicitly (window, swap interval, renderer),
    // so no pending work is owed after it.
    let _ = settings.take_pending_apply();
    Ok((sdl_context, video_subsystem, window, settings, bench))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (sdl_context, video_subsystem, mut window, mut settings, mut bench) = bootstrap()?;

    let mut level_manager = loader::LevelManager::new();
    log_asset_catalog(&level_manager);
    let initial_level = level_manager
        .load_default()
        .map_err(|e| format!("Failed to load initial level: {e}"))?;
    let mut renderer =
        create_renderer(&window, &video_subsystem, &initial_level, &settings, &bench)?;
    configure_vsync(&video_subsystem, &mut bench, &settings);

    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| format!("Failed to init event pump: {e}"))?;

    let mut input_handler = InputHandler::new();
    let (mut game, mut spawn_pos, mut spawn_yaw) = new_game(&initial_level);
    log_prop_usage(&renderer);
    // The current level stays resident for the session: the renderer keeps only
    // GPU state, so a live quality or lightmap change rebuilds from this
    // definition (and the session texture caches) instead of re-reading a file.
    let mut current_level = Some(initial_level);
    let mut display_status = DisplayStatus::default();

    apply_level_request(
        &level_manager,
        &mut renderer,
        &mut game,
        &bench,
        &mut spawn_pos,
        &mut spawn_yaw,
        &mut current_level,
    );
    // `LIMINAL_SPAWN=x,z,yaw_degrees` (or `x,y,z,yaw_degrees`) overrides the
    // level's spawn point, so a hardware run can stand in front of a specific
    // prop instead of walking there with a pad.
    if let Some(spawn_override) = spawn_override_from_env() {
        apply_spawn_override(&mut game, &mut spawn_pos, &mut spawn_yaw, spawn_override);
    }
    // `LIMINAL_STATE_LOG=file.csv` records `frame,x,y,z,yaw,pitch` while the
    // game runs, so control and movement checks can assert real input results
    // from a running build instead of inferring them from screenshots.
    let mut state_log = open_state_log();

    let mut ui_state = new_ui_state(&level_manager);
    let mut perf_overlay = PerfOverlay::new();
    let mut ui_cache = UiGeometryCache::new();
    let mut ui_scratch: Vec<Vertex> = Vec::new();
    let mut applied_filtering = settings.texture_filtering.clone();
    let mut capture_path = capture_path_from_env();
    let capture_at_frame = capture_frame_from_env();

    let mut frame_loop = FrameLoop {
        window: &mut window,
        video_subsystem: &video_subsystem,
        event_pump: &mut event_pump,
        renderer: &mut renderer,
        level_manager: &mut level_manager,
        game: &mut game,
        bench: &mut bench,
        input_handler: &mut input_handler,
        settings: &mut settings,
        ui_state: &mut ui_state,
        perf_overlay: &mut perf_overlay,
        ui_cache: &mut ui_cache,
        ui_scratch: &mut ui_scratch,
        applied_filtering: &mut applied_filtering,
        capture_path: &mut capture_path,
        capture_at_frame,
        state_log: &mut state_log,
        spawn_pos: &mut spawn_pos,
        spawn_yaw: &mut spawn_yaw,
        current_level: &mut current_level,
        display_status: &mut display_status,
        window_apply_in_flight: false,
        last_logged_window: (0, 0, 0, 0),
    };
    frame_loop.run();

    if bench.enabled() {
        bench.finish();
    }

    Ok(())
}

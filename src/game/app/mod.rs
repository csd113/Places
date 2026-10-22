//! Native game application: startup, window, loading, input and frame coordination.

mod actions;
mod audio_runtime;
mod authoring_trace;
mod carry_runtime;
mod console_input;
mod flashlight;
mod item_input;
mod item_runtime;
mod loading;
mod ragdoll_runtime;
mod video;

use crate::engine::{diagnostics, level, loader, logging, perf, quality, render};
use crate::game::perf_overlay::PerfOverlay;
use crate::game::{self, bench, console, display, input, settings, ui, world_labels};

use std::cmp::Ordering;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glam::Vec3;
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use sdl3::mouse::{MouseButton, MouseWheelDirection};
use sdl3::video::{Display, FullscreenType, Window};
use sdl3::{EventPump, Sdl, VideoSubsystem};

use bench::Bench;
use display::DisplayStatus;
use game::{AppState, CollisionWorld, Game};
use input::{InputHandler, MenuNavEvent, ScriptedHold, keycode_to_str, parse_move_script};
use render::{DrawableSize, GraphicsTransition, Renderer, Vertex};
use settings::{Settings, WindowMode};
use ui::{
    SettingsAction, SettingsPage, UiGeometryCache, UiState, activate_settings_item,
    activate_settings_row,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often `PLACES_STATE_LOG` records the player state, in frames.
const STATE_LOG_INTERVAL: u64 = 5;

/// Items in the fixed main menu: Explore, Settings, Credits, Exit.
const MAIN_MENU_ITEM_COUNT: usize = 4;
static MENU_SHUFFLE_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
static LAST_MENU_VIGNETTE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Items in the fixed pause menu: Resume, Settings, Return to Main Menu.
const PAUSE_MENU_ITEM_COUNT: usize = 3;

/// Items after the installed levels: Import, Open Import Folder, Back.
const LEVEL_SELECT_TRAILING_ITEMS: usize = 3;

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

/// Prints what the level's props and baked lighting cost: decoded models,
/// texture memory, draw calls, level-build time and the baked room baselines.
///
/// Developer telemetry: only printed when `PLACES_VERBOSE` is set.
// Startup CLI output that has no logger to route through.
#[expect(
    clippy::print_stdout,
    reason = "Startup CLI output that has no logger to route through."
)]
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
        "[spatial] {} static batch(es) (floor {} / ceiling {} / wall {} / light {} / prop box {} / decal {}), {} prop batch(es)",
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
        level.lightmap_texels.saturating_mul(16) / 1024,
        level.lightmap_millis,
        if level.lightmap_fallback {
            " (fallback: vertex lighting)"
        } else {
            ""
        }
    );
    log_dynamic_scene(renderer);
}

/// Logs the neutral dynamic scene's size.
///
/// Printed at startup and after every level switch, so the demonstration
/// objects a level spawns (and the previous level's objects being cleared) are
/// visible in the developer telemetry.
// Startup CLI output that has no logger to route through.
#[expect(
    clippy::print_stdout,
    reason = "Startup CLI output that has no logger to route through."
)]
fn log_dynamic_scene(renderer: &Renderer) {
    if !logging::verbose() {
        return;
    }
    let (doors, doors_drawn) = renderer.door_render_counts();
    println!(
        "[dynamic] {} object(s): {} draw call(s), {} vertices (the demonstration path; never part of the static bake); doors: {doors_drawn}/{doors}",
        renderer.dynamic_scene().len(),
        renderer.dynamic_scene().draw_count(),
        renderer.dynamic_scene().vertex_count()
    );
}

/// Parses `PLACES_SPAWN` overrides: `x,z,yaw_degrees` keeps the default eye
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

/// Logs the same paths used by all resource and storage consumers.
fn log_package() -> Result<(), String> {
    let assets = crate::engine::paths::assets()?;
    let user = crate::engine::paths::user()?;
    logging::event(format!(
        "[paths] assets={} data={} config={} cache={} import={} projects={}",
        assets.display(),
        user.data.display(),
        user.config.display(),
        user.cache.display(),
        user.imports().display(),
        user.projects().display()
    ));
    Ok(())
}

/// The usable work area of one display, in logical pixels.
///
/// `(0, 0)` when the backend cannot report it, which the pure fitting rule
/// treats as "unknown" rather than "nothing fits". `Display` is a small `Copy`
/// handle, so it is taken by value.
fn usable_display_bounds(display: Display) -> (u32, u32) {
    display
        .get_usable_bounds()
        .map_or((0, 0), |rect| (rect.width(), rect.height()))
}

/// The full resolution of one display, in logical pixels.
///
/// `(0, 0)` when the backend cannot report it; the Display screen then shows
/// "Follows Display" instead of a stale number.
fn display_bounds(display: Display) -> (u32, u32) {
    display
        .get_bounds()
        .map_or((0, 0), |rect| (rect.width(), rect.height()))
}

/// The window's display, falling back to the primary one.
///
/// SDL3 identifies displays with opaque `SDL_DisplayID`s rather than a small
/// integer index; a window that cannot report its display therefore falls back
/// explicitly, never to an invalid zero id.
fn window_display(window: &Window, video: &VideoSubsystem) -> Option<Display> {
    window
        .get_display()
        .or_else(|_| video.get_primary_display())
        .ok()
}

/// Creates the game window from the effective display settings.
///
/// The window opens at the saved windowed resolution ([`crate::game::settings::DEFAULT_WINDOW_WIDTH`]
/// x [`crate::game::settings::DEFAULT_WINDOW_HEIGHT`] on a fresh install), reduced to
/// fit the active display's work area when necessary, or as borderless
/// fullscreen when that is the selected mode. `high_pixel_density` keeps the
/// drawable at the Retina backing scale; the renderer reads the pixel size,
/// never the logical size.
///
/// A size that had to be reduced to fit is adopted back into `settings` (and
/// therefore persisted and shown in Display), so the menu and the window never
/// disagree.
fn create_window(video: &VideoSubsystem, settings: &mut Settings) -> Result<Window, String> {
    let requested = settings.window_size();
    let usable = video
        .get_primary_display()
        .map_or((0, 0), usable_display_bounds);
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

    let mut builder = video.window("Places", width, height);
    let _configured_position_centered =
        builder.position_centered().resizable().high_pixel_density();
    if settings.window_mode() == WindowMode::Fullscreen {
        // A window asked for fullscreen without an explicit display mode is
        // borderless desktop fullscreen.
        let _configured_fullscreen = builder.fullscreen();
    }
    let window = builder
        .build()
        .map_err(|e| format!("Failed to create window: {e}"))?;

    if settings.window_mode() == WindowMode::Fullscreen {
        // SDL3 applies window state asynchronously: the fullscreen request made
        // through the builder is finalized after the window exists, so the boot
        // frame, the drawable the renderer configures and a first-frame capture
        // would all otherwise see the windowed size. `SDL_SyncWindow` is the
        // documented barrier for pending window state; without it the first
        // frames render (and a capture would take) the windowed size and the
        // window visibly resizes once the request lands. A timed-out sync is
        // not fatal: the request is still pending and the per-frame display
        // poll adopts whatever the platform reports.
        if !window.sync() {
            logging::warn(
                "[display] fullscreen did not finalize before the first frame; \
                 the window will resize when the platform applies it"
                    .to_string(),
            );
        }
    }

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
    let sdl_context = sdl3::init().map_err(|e| format!("Failed to init SDL3: {e}"))?;
    let video_subsystem = sdl_context
        .video()
        .map_err(|e| format!("Failed to init video subsystem: {e}"))?;

    let window = create_window(&video_subsystem, settings)?;

    // SDL3 starts with text input disabled and scopes it per window. This
    // game consumes only raw key events, never composed text, so stopping text
    // input explicitly keeps the platform IME away from the keyboard path at no
    // behavioural cost.
    video_subsystem.text_input().stop(&window);
    Ok((sdl_context, video_subsystem, window))
}

/// Logs one startup line for the asset catalog: how many logical assets it
/// declares and which environment themes organize them. Lookup itself is
/// resolved once per level load, never per frame.
///
/// Developer telemetry: only printed when `PLACES_VERBOSE` is set.
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
    let available_themes = if themes.is_empty() {
        "(none)".to_string()
    } else {
        themes.join(", ")
    };
    logging::info(format!(
        "[assets] {} placeable asset(s) of {} catalog entries; themes: {available_themes}",
        catalog.len(),
        catalog.assets().len()
    ));
}

/// Spawns dynamic fixture parts and effects from the installed content.
fn spawn_level_demonstration(renderer: &mut Renderer, loaded: &loader::LoadedLevel) {
    let _dynamic_demo_changed = renderer.set_dynamic_demo(&loaded.level);
    // Floating props are level content, not a demonstration: every level that
    // authors `float` gets them, and the spawn is idempotent. It runs after
    // the demo spawn because that one clears the whole dynamic scene first.
    let _floating_props_changed = renderer.set_floating_props(&loaded.level);
    // Ambient effects are level content too: the install already built the
    // steam plumes from the resolved material table, and this idempotent call
    // is the engine's explicit hand-off.
    let _level_effects_changed = renderer.set_level_effects(&loaded.level);
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
    settings: &Settings,
    bench: &Bench,
) -> Result<Renderer, String> {
    let mut renderer =
        Renderer::new(window).map_err(|e| format!("Failed to initialize the renderer: {e}"))?;
    perf::startup_mark("renderer device");
    // The requested graphics configuration is recorded before the first level
    // upload, so the load itself bakes the right atlas, fits the right texture
    // budgets and creates the right reflection targets. The quality level, the
    // Lightmaps and the Reflections setting are independent (see
    // `docs/RENDERER.md` §12), so `Low + Lightmaps Full` loads a Full atlas
    // even though the overall level is Low.
    renderer.set_quality(settings.quality_level());
    renderer.set_lighting_quality(settings.lighting_quality());
    renderer.set_lightmap_quality(settings.lightmap_quality());
    renderer.set_reflection_quality(settings.reflection_quality());
    // Bloom and Texture Filtering are independent player preferences too (with
    // their `PLACES_*` startup overrides already folded in). Filtering is
    // recorded before the level upload so the load-time texture diagnostic
    // names the preset actually in force.
    renderer.set_bloom_enabled(settings.bloom_enabled());
    renderer.set_texture_filtering(settings.texture_filtering_preset());
    renderer.set_culling(!bench.no_cull());
    #[cfg(feature = "visual-diagnostics")]
    if let Ok(selector) = std::env::var("PLACES_VISUAL_DIAGNOSTIC") {
        renderer.set_visual_diagnostic(&selector)?;
    }
    Ok(renderer)
}

/// Applies the effective `VSync` setting through the renderer.
///
/// The renderer translates the preference into a supported presentation mode.
/// The same helper is called at runtime whenever the player changes the
/// setting, so `VSync` applies immediately instead of at the next launch.
fn configure_vsync(renderer: &mut Renderer, bench: &mut Bench, settings: &Settings) {
    let swap_interval = renderer.set_swap_interval(settings.vsync_enabled());
    bench.set_reported_swap_interval(swap_interval);
}

/// Display names of the installed levels, in discovery order.
fn level_entry_names(level_manager: &loader::LevelManager) -> Vec<String> {
    let mut projects = std::collections::BTreeMap::new();
    level_manager
        .menu_entries()
        .iter()
        .map(|entry| {
            let prefix = projects.entry(&entry.path).or_insert_with(|| {
                crate::engine::package::project::inspect(&entry.path)
                    .ok()
                    .flatten()
                    .map(|project| format!("{} / ", project.project.name))
            });
            prefix
                .as_deref()
                .and_then(|project_prefix| entry.name.strip_prefix(project_prefix))
                .unwrap_or(&entry.name)
                .to_string()
        })
        .collect()
}

fn settings_save_problem(error: &std::io::Error) -> String {
    let path =
        Settings::default_path().map_or_else(|problem| problem, |path| path.display().to_string());
    format!(
        "Your preferences could not be saved.\nCheck that the settings file and its folder are writable, then try again.\n\nSettings file:\n{path}\n\nTechnical details:\n{error}"
    )
}

/// UI state seeded with the installed level names.
fn new_ui_state(level_manager: &loader::LevelManager) -> UiState {
    UiState {
        level_entries: level_entry_names(level_manager),
        import_directory: crate::engine::paths::user()
            .map_or_else(|error| error, |paths| paths.imports().display().to_string()),
        ..UiState::new()
    }
}

/// The one-shot framebuffer capture path from `PLACES_CAPTURE`, if set.
fn capture_path_from_env() -> Option<PathBuf> {
    std::env::var("PLACES_CAPTURE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The frame `PLACES_CAPTURE` should be taken on, 1-based.
///
/// The default is the first rendered frame, which is what every existing
/// capture does. `PLACES_CAPTURE_FRAME=n` waits for frame `n` first, so a
/// capture can show something that changes over time (the washer-drum
/// demonstration turns as the frame loop runs) without a second launch.
fn capture_frame_from_env() -> u64 {
    std::env::var("PLACES_CAPTURE_FRAME")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|frame| *frame > 0)
        .unwrap_or(0)
}

/// The ready-world simulation second `PLACES_CAPTURE` should be taken on.
///
/// `PLACES_CAPTURE_TIME=seconds` waits for the first ready frame at or after
/// that accumulated simulation time, exactly like `PLACES_CAPTURE_FRAME` but
/// frame-rate independent, so a scripted capture shows the same in-world
/// moment on a 55 fps and a 120 fps run.
fn capture_time_from_env() -> Option<f32> {
    std::env::var("PLACES_CAPTURE_TIME")
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
}

/// The `PLACES_INTERACT` developer overrides: authored instance ids whose
/// interaction is dispatched once each, on the first Playing frame of a
/// committed world. It exists so a capture run can drive a switch without
/// input scripting; ordinary play never sets it.
fn interact_overrides_from_env() -> Vec<String> {
    std::env::var("PLACES_INTERACT")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The `PLACES_MOVE_SCRIPT` developer override: held controls for a run.
///
/// Entries are `control@first-last` in ready-world simulation seconds
/// (inclusive) and are applied without touching gameplay state, so a capture
/// run can drive the player through a real jump or pool crossing. Unknown or
/// malformed entries are rejected with one message; ordinary play never sets
/// the variable.
fn move_script_from_env() -> Vec<ScriptedHold> {
    let Some(value) = std::env::var("PLACES_MOVE_SCRIPT").ok() else {
        return Vec::new();
    };
    let (script, rejected) = parse_move_script(&value);
    if !rejected.is_empty() {
        crate::engine::logging::warn(format!(
            "PLACES_MOVE_SCRIPT: ignored {} malformed entr{}: {}",
            rejected.len(),
            if rejected.len() == 1 { "y" } else { "ies" },
            rejected.join(", ")
        ));
    }
    script
}

/// The parsed `PLACES_SPAWN` override, if set.
fn spawn_override_from_env() -> Option<[f32; 4]> {
    std::env::var("PLACES_SPAWN")
        .ok()
        .and_then(|value| parse_spawn_override(&value))
}

/// Opens the `PLACES_STATE_LOG` CSV file, reporting why it cannot be used.
// Startup CLI output that has no logger to route through.
#[expect(
    clippy::print_stderr,
    reason = "Startup CLI output that has no logger to route through."
)]
fn open_state_log() -> Option<std::fs::File> {
    let path = std::env::var("PLACES_STATE_LOG")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| eprintln!("PLACES_STATE_LOG: cannot open {path}: {error}"))
        .ok()
}

/// Applies a `PLACES_SPAWN` override.
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
    game.reset_spawn_point(*spawn_pos, *spawn_yaw);
}

/// Opens one screen on the first frame for a capture, from `PLACES_SCREEN`.
///
/// Developer diagnostic like `PLACES_PAUSE`: `settings`, `graphics` or
/// `advanced` (the Graphics page with the Advanced group expanded) opens that
/// screen so the one-shot capture can show it without a keyboard. Inert unless
/// the variable names a screen; it changes nothing about how a screen draws.
fn apply_screen_request(game: &mut Game, ui_state: &mut UiState) {
    let Ok(request) = std::env::var("PLACES_SCREEN") else {
        return;
    };
    match request.trim().to_ascii_lowercase().as_str() {
        "explore" => game.set_app_state(AppState::LevelSelect),
        "settings" => {
            ui_state.settings_page = SettingsPage::Root;
            game.set_app_state(AppState::Settings);
        }
        "graphics" | "advanced" => {
            ui_state.settings_page = SettingsPage::Graphics;
            ui_state.advanced_expanded = request.trim().eq_ignore_ascii_case("advanced");
            game.set_app_state(AppState::Settings);
        }
        _ => {}
    }
}

/// True when `PLACES_PAUSE` asks for the pause menu on the first frame.
fn pause_requested() -> bool {
    std::env::var("PLACES_PAUSE").is_ok_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off"
        )
    })
}

/// Renders one frame of the running level to `path` as a PNG.
///
/// The `PLACES_CAPTURE=frame.png` developer path captures rendered pixels
/// directly for reproducible visual checks.
// CLI output that has no logger to route through.
#[expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI output that has no logger to route through."
)]
fn write_capture(renderer: &mut Renderer, path: &Path) {
    match renderer.capture_default_framebuffer() {
        Ok(image) => match loader::encode_png(&image) {
            Ok(bytes) => match std::fs::write(path, bytes) {
                Ok(()) => println!("PLACES_CAPTURE: wrote {}", path.display()),
                Err(error) => {
                    eprintln!("PLACES_CAPTURE: cannot write {}: {error}", path.display());
                }
            },
            Err(error) => eprintln!("PLACES_CAPTURE: {error}"),
        },
        Err(error) => eprintln!("PLACES_CAPTURE: {error}"),
    }
}

/// Every piece of mutable state the frame loop touches.
///
/// Grouping them keeps `main` a readable setup sequence and lets each step of a
/// frame be reviewed (and unit-tested) on its own.
#[derive(Clone, Copy, Debug)]
enum LoadIntent {
    Background,
    Play,
    Graphics,
}

struct PendingCommit {
    id: u64,
    loaded: loader::LoadedLevel,
    collision: CollisionWorld,
    intent: LoadIntent,
    lease: loading::PreparedLease,
}

/// Where startup is in bringing up the first world.
///
/// Optional initial-world preparation precedes the ordinary menu. An empty
/// installation or a failed package reveals the usable menu without a world.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupPhase {
    /// First world requested and not yet committed: menu hidden, input gated.
    Preparing,
    /// The ordinary menu experience applies, with or without a world.
    Ready,
}

#[derive(Default)]
struct PresentationState {
    has_presented: bool,
    pending_scene: Option<u64>,
}

struct FrameLoop<'a> {
    diagnostic_tick: Instant,
    diagnostic_state: Option<AppState>,
    window: &'a mut Window,
    /// The SDL context, for the mouse-capture reconciliation.
    sdl: &'a Sdl,
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
    console: console::Console,
    console_runtime: console::runtime::Runtime,
    flashlight: flashlight::Power,
    outcome: Option<(bool, String, String)>,
    message: Option<(String, Vec<String>)>,
    message_page: usize,
    items: item_runtime::Items,
    audio: audio_runtime::Audio,
    video: Option<video::Recording>,
    ui_cache: &'a mut UiGeometryCache,
    ui_scratch: &'a mut Vec<Vertex>,
    applied_filtering: &'a mut String,
    capture_path: &'a mut Option<PathBuf>,
    capture_at_frame: u64,
    /// `PLACES_CAPTURE_TIME`'s ready-world simulation second, when set; it
    /// takes precedence over the frame count.
    capture_at_seconds: Option<f32>,
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
    /// True while the window has keyboard focus; a focused window is what
    /// allows relative mouse mode during gameplay. Starts assumed focused so a
    /// boot straight into `PLACES_LEVEL` captures as soon as Playing begins.
    window_focused: bool,
    /// Set when the active backend reported a fatal GPU error: the loop stops
    /// and `main` exits with this message.
    fatal_error: Option<String>,
    loader: loading::Loader,
    load_intent: Option<LoadIntent>,
    pending_commit: Option<PendingCommit>,
    /// Active-world ownership is independent of the worker's bounded LRU.
    displayed_lease: Option<loading::PreparedLease>,
    retired_worlds: std::collections::VecDeque<(String, loading::LifetimeProbe)>,
    load_source: Option<loading::Source>,
    load_generation: u64,
    /// Lightmap quality the in-flight preparation was requested with, so an
    /// equivalent request can reuse it and a changed quality cannot.
    load_lightmaps: quality::LightmapQuality,
    load_return_state: AppState,
    load_phase: Option<loading::Phase>,
    /// Where startup stands on bringing up the first world (see
    /// [`StartupPhase`]). The ordinary menu is not offered before it is
    /// `Ready`: the window keeps pumping events and showing the preparation
    /// screen, and the first normal menu frame already draws the requested
    /// level behind it.
    startup: StartupPhase,
    launch_overrides: bool,
    ready_frames: u64,
    presentation: PresentationState,
    trace: perf::loading::LoadTrace,
    actions: Option<actions::Actions>,
    /// The graphics request that most recently failed to apply.
    ///
    /// The player's selection stands (it is never rewritten to hide a failed
    /// transition); this only keeps the state-driven reconciliation from
    /// retrying the same request every frame. An explicit selection clears it.
    failed_graphics: Option<quality::GraphicsSpec>,
    /// The exact "Applying ..." hint currently owned by
    /// [`Self::refresh_graphics_status_hint`], so it is cleared only when it is
    /// still the message on screen.
    graphics_hint: Option<String>,
    /// `PLACES_NAV_DEBUG`'s directory, when set: a developer dump of the live
    /// navigation mesh and AI state is written there, from the real runtime
    /// data. Unset means no work at all.
    nav_debug_dir: Option<PathBuf>,
    /// Frames since the last AI state dump.
    nav_debug_age: u64,
    /// `PLACES_INTERACT`'s instance ids, dispatched one by one on the first
    /// Playing frame after a world commit. Empty in ordinary play.
    dev_interactions: Vec<String>,
    /// `PLACES_MOVE_SCRIPT`'s control holds, applied against the ready-world
    /// simulation seconds before the player update. Empty in ordinary play.
    move_script: Vec<ScriptedHold>,
    /// Simulation seconds accumulated since the world became ready, the clock
    /// `move_script` is scheduled against.
    ready_seconds: f32,
}

impl FrameLoop<'_> {
    /// Requests the first world and runs the frame loop to completion.
    fn start(&mut self, initial_entry: Option<loader::LevelEntry>, direct: bool) {
        if let Some(entry) = initial_entry {
            self.request_load(
                loading::Source::entry(entry),
                if direct {
                    LoadIntent::Play
                } else {
                    LoadIntent::Background
                },
            );
        } else {
            self.startup = StartupPhase::Ready;
            self.trace.record("menu_ready", 0, "no initial map");
        }
        self.run();
    }

    /// Runs frames until the game stops.
    fn run(&mut self) {
        while self.game.is_running() {
            self.frame();
        }
        self.audio.system.clear_world();
        self.trace.record(
            "audio_shutdown",
            self.load_generation,
            &serde_json::to_string(&self.audio.system.stats()).unwrap_or_default(),
        );
        self.audio.finish();
        self.loader.shutdown();
        if let Some(actions) = &mut self.actions {
            actions.shutdown();
        }
        self.trace
            .record("shutdown_requested", self.load_generation, "");
        while !self.loader.is_finished()
            || self
                .actions
                .as_ref()
                .is_some_and(|actions| !actions.is_finished())
        {
            self.pump_events();
            if self.fatal_error.is_some() {
                std::thread::sleep(std::time::Duration::from_millis(16));
                continue;
            }
            self.game.reset_timing();
            self.ui_state
                .set_status("Stopping level preparation...".to_string(), false);
            let (width, height) = self.window.size_in_pixels();
            let _drawable_size_changed = self
                .renderer
                .set_drawable_size(DrawableSize::new(width, height));
            self.renderer.render_scene(render::RenderCamera::new(
                *self.spawn_pos,
                *self.spawn_yaw,
                0.0,
                self.settings.fov_degrees,
            ));
            let vertices = self.ui_cache.get(
                AppState::MainMenu,
                self.ui_state,
                self.settings,
                self.display_status,
                APP_VERSION,
            );
            self.renderer.render_ui(vertices);
            let _present_status = self.renderer.present(self.window);
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        if let Err(error) = self.loader.join_finished() {
            self.fatal_error = Some(error);
        }
        if let Some(actions) = &mut self.actions
            && let Err(error) = actions.join_finished()
        {
            self.fatal_error = Some(error);
        }
        self.trace
            .record("shutdown_complete", self.load_generation, "");
    }

    fn record_menu_beat(&mut self) {
        if !self.menu_world() {
            return;
        }
        if let Some(crate::engine::entities::components::StateValue::Int(value)) = self
            .game
            .entities()
            .handle_of("guest")
            .and_then(|handle| self.game.entities().components().states.get(handle))
            .and_then(|state| state.get("vignette"))
            && (1..=12).contains(value)
        {
            LAST_MENU_VIGNETTE.store(*value, std::sync::atomic::Ordering::Relaxed);
        }
        if !self.trace.is_recording() {
            return;
        }
        let beat = self
            .game
            .entities()
            .handle_of("guest")
            .and_then(|handle| self.game.entities().components().states.get(handle))
            .and_then(|state| state.get("beat"))
            .and_then(|state| {
                if let crate::engine::entities::components::StateValue::Int(value) = state {
                    Some(*value)
                } else {
                    None
                }
            });
        if beat != self.ui_state.observed_menu_beat {
            self.ui_state.observed_menu_beat = beat;
            self.trace_world("menu_beat");
        }
    }

    fn record_diagnostic_context(&mut self) {
        self.record_menu_beat();
        if self.diagnostic_state != Some(self.game.app_state())
            || self.diagnostic_tick.elapsed() >= std::time::Duration::from_secs(15)
        {
            self.diagnostic_state = Some(self.game.app_state());
            self.diagnostic_tick = Instant::now();
            self.trace_world("runtime_context");
        }
    }

    /// One complete frame: input, simulation, render, present, telemetry.
    // one cohesive frame: input, update, draw
    fn frame(&mut self) {
        self.record_diagnostic_context();
        // A fatal GPU condition must stop the process through the normal
        // shutdown path instead of issuing more work on a lost device.
        if self.stop_on_fatal_renderer_error() {
            return;
        }
        // Frame boundary for the benchmark harness: everything from here to the
        // end of the buffer swap is one complete frame, swap included.
        let frame_begin = Instant::now();
        // The cadence baseline advances every loop, sampled or not: loading and
        // transition frames are excluded from the steady-state samples but must
        // not be charged to the next sample as one long interval.
        self.bench.begin_frame(frame_begin);
        self.game.update_timing();
        self.perf_overlay.set_gpu_timing(self.renderer.gpu_timing());
        self.perf_overlay.update(self.game.delta_seconds());

        self.trace.record("event_pump", self.load_generation, "");
        self.pump_events();
        if !self.game.is_running() {
            return;
        }
        // Relative mouse mode follows the app state and window focus exactly;
        // the reconciliation is one per frame and only calls SDL on a change.
        self.sync_mouse_capture();

        if self.input_handler.quit_requested() {
            self.game.stop();
        }

        // Apply whatever the player changed in Settings before this frame
        // simulates or draws, then refresh what the windowing backend actually
        // did (a window change, a monitor move, a HiDPI backing-scale change).
        // A scripted benchmark quality cycle goes through the same path, so a
        // `Full -> Low -> Full` run exercises the renderer's real rebuild; a
        // scripted window action goes through the real SDL window, so resize
        // and minimize events reach the same drawable path a manual resize
        // does.
        if self.bench.enabled()
            && self.load_intent.is_none()
            && self.current_level.is_some()
            && let Some(level) = self
                .bench
                .quality_cycle_at(self.ready_frames.saturating_add(1))
        {
            let _quality_changed = self.settings.set_quality(level);
        }
        if self.bench.enabled()
            && self.load_intent.is_none()
            && self.current_level.is_some()
            && let Some(change) = self
                .bench
                .graphics_cycle_at(self.ready_frames.saturating_add(1))
        {
            self.apply_bench_graphics_change(change);
        }
        if self.bench.enabled()
            && self.load_intent.is_none()
            && self.current_level.is_some()
            && let Some(action) = self
                .bench
                .window_cycle_at(self.ready_frames.saturating_add(1))
        {
            self.apply_window_action(action);
        }
        self.apply_pending_settings();
        self.advance_loading();
        // An asynchronous graphics transition (an uncached lightmap fill) can
        // finish on any frame, so the subtle status hint is refreshed every
        // frame; it is two enum checks when nothing is in flight.
        self.refresh_graphics_status_hint();
        self.refresh_display_status();
        self.reconcile_console();
        self.sync_menu_pointer();
        self.sync_mouse_capture();
        if self.game.frame_count() == 1 {
            perf::startup_mark("input responsive");
        }

        if self.load_intent.is_some() && self.game.app_state() == AppState::Playing {
            self.game.set_app_state(AppState::Paused);
            self.clear_loading_inputs(matches!(self.load_intent, Some(LoadIntent::Graphics)));
            self.game.reset_timing();
        }
        // Update player movement (only active during AppState::Playing)
        if self.load_intent.is_none()
            && self.current_level.is_some()
            && let Some(delta) = self.bench.fixed_delta_seconds()
        {
            self.game.set_benchmark_simulation_delta(delta);
        }
        if !self.simulation_active() {
            self.game.reset_timing();
        }
        self.renderer.set_visual_style(
            self.settings.visual_style.shader_value(),
            self.ready_seconds,
        );
        self.update_audio_mix();
        let _clock_updated = self
            .game
            .set_world_time_scale(self.items.world.simulation_state().time_scale);
        if !self.console.is_open() {
            self.update_player_input();
        }
        if self.menu_active() {
            self.ready_seconds = (self.ready_seconds + self.game.sim_delta_seconds()).min(1.0e6);
            self.game.update_menu_world();
        }
        self.reconcile_gameplay_reset();
        self.log_player_state();
        // One press is one interaction: consume the edge the movement update
        // latched, resolve the object under the crosshair and run its
        // map-authored actions. `window_focused` gates the key the same way it
        // gates relative mouse mode.
        self.dispatch_player_interactions();
        self.dispatch_dev_interactions();
        self.reconcile_gameplay_reset();
        // Door leaves follow the gameplay state: the drawn slab and the
        // physical collider read the same angle, so a door can never stop the
        // player where it is not drawn.
        self.renderer.sync_doors(self.game.doors());
        // Fixture switches are gameplay state too: hand the renderer any state
        // that changed this frame (usually none) so it can re-light the
        // affected charts and scale the fixture's own face emission.
        let light_toggles = self.game.take_light_toggles();
        if !light_toggles.is_empty() {
            self.renderer.apply_light_toggles(&light_toggles);
        }
        // The entity runtime's spawn/despawn/move/material/audio commands are
        // applied before this frame draws, so a spawn requested by a trigger
        // or interaction is visible on the frame it happened.
        self.apply_live_scene_commands();
        // Advance the dynamic objects (the demonstration drum and any other
        // spawned object) once per frame: transform only, never a geometry or
        // lightmap rebuild.
        self.update_world_presentation();
        self.update_game_audio();
        self.write_nav_debug(false);
        let frame_update_done = Instant::now();

        self.render_and_present(frame_begin, frame_update_done);
    }

    fn crosshair_visible(&self) -> bool {
        self.game.app_state() == AppState::Playing
            && self.load_intent.is_none()
            && self.outcome.is_none()
            && self.message.is_none()
            && !self.console.is_open()
            && !self.console_runtime.representation.is_active()
            && self.current_level.as_ref().is_some_and(|loaded| {
                self.console_runtime
                    .crosshair_enabled(loaded.level.crosshair)
            })
    }

    fn reconcile_gameplay_reset(&mut self) {
        let held = self.items.world.held();
        self.console_runtime.reconcile_player_reset(
            self.game,
            self.renderer,
            &mut self.items.world,
        );
        if held != self.items.world.held() {
            self.items.input.invalidate();
            self.items.target = None;
        }
        if self.reconcile_audio_reset() {
            self.reset_carry_assemblies();
            if self.menu_world() {
                self.reset_menu_items();
            }
            self.flashlight = flashlight::Power::Off;
            self.outcome = None;
            self.message = None;
            self.update_item_triggers(true);
        }
    }

    fn update_world_presentation(&mut self) {
        let player_visual_delta = self.visual_delta_seconds();
        let visual_delta = self.game.scale_world_delta(player_visual_delta);
        let _representation_synced = self.console_runtime.representation.sync(
            self.renderer,
            Vec3::new(
                self.game.player_position.x,
                self.game.feet_y(),
                self.game.player_position.z,
            ),
            self.game.player_yaw,
            self.game.locomotion_snapshot(),
            player_visual_delta,
        );
        self.console_runtime
            .sync_visuals(self.renderer, &self.items.world);
        self.advance_weather();
        // Each actor owns its pose. Completed one-shot cues feed back to
        // entity sequences and animation_complete bindings.
        self.renderer.sync_ragdolls(&self.items.world);
        for (id, position, bounds) in self.items.world.ragdoll_roots() {
            self.game
                .entities_mut()
                .publish_ragdoll(id, position, bounds);
        }
        let characters = self
            .renderer
            .advance_characters(visual_delta, self.game.entity_frames());
        self.update_carry_contacts(visual_delta);
        self.update_carry_attachments(visual_delta);
        // Publish every final pose once. Water, props and characters observe
        // the same visibility, including this frame's socket attachments.
        let _update_stats = self.renderer.update_dynamic(visual_delta);
        self.renderer.sync_characters();
        for instance_id in &characters.finished {
            self.game
                .entities_mut()
                .notify_animation_complete(instance_id, "");
        }
    }

    fn reset_menu_items(&mut self) {
        let groups = self
            .current_level
            .as_ref()
            .map(|loaded| {
                loaded
                    .level
                    .physics_reset_groups()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for group in groups {
            self.reset_item_group(&group);
        }
        self.initialize_menu_choreography();
    }

    fn initialize_menu_choreography(&mut self) {
        if !self.menu_world() {
            return;
        }
        let counter = MENU_SHUFFLE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let fresh = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(counter, |duration| {
                duration.subsec_nanos().wrapping_add(counter)
            });
        let seed = std::env::var("PLACES_MENU_SEED")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(fresh)
            & 0x7fff_ffff;
        let first = std::env::var("PLACES_MENU_FIRST")
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| (1..=12).contains(value))
            .unwrap_or(0);
        let actions = [
            crate::engine::level::ActionDef::SetState {
                target: Some("guest".into()),
                name: "menu_seed".into(),
                value: crate::engine::entities::components::StateValue::Int(i64::from(seed)),
            },
            crate::engine::level::ActionDef::SetState {
                target: Some("guest".into()),
                name: "menu_first".into(),
                value: crate::engine::entities::components::StateValue::Int(first),
            },
            crate::engine::level::ActionDef::SetState {
                target: Some("guest".into()),
                name: "previous_vignette".into(),
                value: crate::engine::entities::components::StateValue::Int(
                    LAST_MENU_VIGNETTE.load(std::sync::atomic::Ordering::Relaxed),
                ),
            },
        ];
        let _published = self.game.entities_mut().dispatch_actions(&actions, None);
    }

    fn menu_world(&self) -> bool {
        self.current_level
            .as_ref()
            .is_some_and(|world| world.level.id == "places_menu")
    }
    fn menu_active(&self) -> bool {
        self.menu_world()
            && self.startup == StartupPhase::Ready
            && self.load_intent.is_none()
            && self.window_focused
            && matches!(
                self.game.app_state(),
                AppState::MainMenu | AppState::LevelSelect | AppState::Settings
            )
    }
    fn simulation_active(&self) -> bool {
        self.gameplay_active() || self.menu_active()
    }

    /// The common clock/input gate; world scaling is applied only after this gate.
    fn gameplay_active(&self) -> bool {
        self.outcome.is_none()
            && self.message.is_none()
            && self.window_focused
            && !self.console.is_open()
            && self.load_intent.is_none()
            && self.game.app_state() == AppState::Playing
    }

    /// Advances weather controls only while the loaded world is playing.
    fn advance_weather(&mut self) {
        let delta = if self.simulation_active() {
            self.game.world_delta_seconds()
        } else {
            0.0
        };
        self.renderer.update_weather(delta);
        self.game
            .world_mut()
            .set_weather_state(self.renderer.weather_state());
    }

    /// Advances controlled visual time only while the diagnostic world is playing.
    fn visual_delta_seconds(&self) -> f32 {
        if !self.simulation_active() {
            return 0.0;
        }
        self.bench
            .fixed_delta_seconds()
            .unwrap_or_else(|| self.game.delta_seconds())
    }

    /// Applies this ready-world frame's `PLACES_MOVE_SCRIPT` holds, if any.
    ///
    /// The script drives the player through real held controls with real press
    /// edges, in the ordinary input state, so a capture run can record a jump
    /// or a pool crossing without touching gameplay state; it is empty in
    /// ordinary play. Scheduling is by accumulated simulation seconds, so the
    /// same script produces the same in-world sequence at any frame rate.
    fn apply_move_script(&mut self) {
        if self.startup != StartupPhase::Ready || !self.gameplay_active() {
            return;
        }
        self.ready_seconds = (self.ready_seconds + self.game.sim_delta_seconds()).min(1.0e6);
        self.input_handler
            .state_mut()
            .apply_move_script(&self.move_script, self.ready_seconds);
    }

    /// Dispatches the `PLACES_INTERACT` ids once each, on the first Playing
    /// frame after a world commit. Developer diagnostic; empty by default.
    fn dispatch_dev_interactions(&mut self) {
        if self.dev_interactions.is_empty()
            || self.game.app_state() != AppState::Playing
            || self.startup != StartupPhase::Ready
        {
            return;
        }
        for id in std::mem::take(&mut self.dev_interactions) {
            let Some(index) = self.game.interactables().index_of(&id) else {
                crate::engine::logging::warn_once(
                    format!("interact-override:{id}"),
                    format!("PLACES_INTERACT: `{id}` is not interactable in this level"),
                );
                continue;
            };
            if let Some(report) = self.game.world_mut().dispatch_interaction(Some(index)) {
                self.interaction_audio_feedback(index, &report);
                crate::engine::logging::info(format_args!(
                    "PLACES_INTERACT: `{id}` ran {} action(s), spawned {}, unsupported {}",
                    report.actions_run, report.spawned, report.unsupported
                ));
            }
        }
    }

    /// Runs the current interaction and reports what it did.
    ///
    /// The dispatcher itself is engine state ([`Game`]); this only narrates
    /// the outcome to the developer log, and never reaches into the renderer.
    fn dispatch_interaction(&mut self) {
        self.observe_held_item();
        if self.dispatch_item_interaction() {
            return;
        }
        let Some(item_runtime::InteractionTarget::World { index, distance: _ }) = self.items.target
        else {
            return;
        };
        let Some(report) = self.game.dispatch_interaction_at(Some(index)) else {
            return;
        };
        self.interaction_audio_feedback(index, &report);
        if report.player_reset {
            crate::engine::logging::info(
                "[interact] reset_to_start: returned to the authored spawn",
            );
        } else if report.labels_toggled() > 0 {
            let verb = if report.labels_shown > 0 {
                "shown"
            } else {
                "hidden"
            };
            crate::engine::logging::info(format!("[interact] label {verb}"));
        } else if report.animations_started > 0 {
            crate::engine::logging::info("[interact] animation cue set");
        } else if report.spawned > 0 {
            crate::engine::logging::info("[interact] entity spawned");
        } else if report.unsupported > 0 {
            crate::engine::logging::warn(
                "[interact] action is not implemented; validation should have rejected it",
            );
        } else if report.missing_targets > 0 {
            crate::engine::logging::warn("[interact] action target names no placed instance");
        }
    }

    /// Applies the entity runtime's render and audio commands.
    ///
    /// The world never names a GPU object: it emits typed commands keyed by a
    /// stable runtime key, and this is the one place they become renderer
    /// calls. A failed spawn is counted in the world and reported once per
    /// model here, never retried in a loop.
    #[expect(
        clippy::too_many_lines,
        reason = "one command dispatcher, one arm per command"
    )] // one command dispatcher, one arm per command
    fn apply_world_commands(&mut self) {
        let commands = self.game.entities_mut().take_commands();
        for command in commands {
            match command {
                crate::engine::entities::WorldCommand::ShowMessage { title, lines } => {
                    if self.message.is_none() && self.outcome.is_none() {
                        self.message_page = 0;
                        self.message = Some((title, lines));
                        self.input_handler.clear_gameplay_inputs();
                        self.items.input.invalidate();
                        self.items.target = None;
                        self.game.reset_timing();
                        self.trace_world("guidance_shown");
                    }
                }
                crate::engine::entities::WorldCommand::EndRun { won, title, detail } => {
                    if self.outcome.is_none() {
                        self.outcome = Some((won, title, detail));
                        self.input_handler.clear_gameplay_inputs();
                        self.items.input.invalidate();
                        self.items.target = None;
                        self.game.reset_timing();
                        self.trace_world("run_finished");
                    }
                }
                crate::engine::entities::WorldCommand::ChangeLevel { level } => {
                    if let Some(path) = self
                        .current_level
                        .as_ref()
                        .map(|loaded| loaded.entry.path.clone())
                        && let Some(index) = self
                            .level_manager
                            .entries()
                            .iter()
                            .position(|entry| entry.path == path && entry.id == level)
                    {
                        self.load_level_at(index);
                        break;
                    }
                    crate::engine::logging::warn(format!(
                        "Transition destination '{level}' is unavailable in this package"
                    ));
                }
                crate::engine::entities::WorldCommand::HandoffItem { target } => {
                    self.handoff_item(&target);
                }
                crate::engine::entities::WorldCommand::ReleaseItem { target } => {
                    self.release_carried_item(&target);
                }
                crate::engine::entities::WorldCommand::RetireHidden { target } => {
                    self.request_carry_retirement(&target);
                }
                crate::engine::entities::WorldCommand::Ragdoll {
                    target,
                    enabled,
                    velocity,
                } => {
                    self.apply_ragdoll(&target, enabled, Vec3::from_array(velocity));
                }
                crate::engine::entities::WorldCommand::ActivatePhysicsGroup { group } => {
                    self.activate_item_group(&group);
                }
                crate::engine::entities::WorldCommand::ResetPhysicsGroup { group } => {
                    self.reset_item_group(&group);
                }
                crate::engine::entities::WorldCommand::SetWeatherAlternate { alternate } => {
                    let _weather_changed = self.renderer.set_weather_alternate(alternate);
                }
                crate::engine::entities::WorldCommand::SetWeatherStrength {
                    strength,
                    transition_seconds,
                } => {
                    let _weather_changed = self
                        .renderer
                        .set_weather_strength(strength, transition_seconds);
                }
                crate::engine::entities::WorldCommand::SetWeatherCycle { enabled } => {
                    let _weather_changed = self.renderer.set_weather_cycle(enabled);
                }
                crate::engine::entities::WorldCommand::SpawnDynamic {
                    entity,
                    model,
                    position,
                    yaw_degrees,
                    scale,
                } => {
                    // A model with a rig renders through the character path so
                    // it animates; anything else stays a dynamic object. Both
                    // lanes key off the same entity lifecycle.
                    let instance_id = self
                        .game
                        .entities()
                        .handle_for_dynamic_key(entity)
                        .and_then(|handle| self.game.entities().instance_id_of(handle))
                        .map(str::to_string);
                    let mut spawned_character = false;
                    if let Some(character_id) = instance_id.as_deref() {
                        match self.renderer.spawn_runtime_character(
                            character_id,
                            &model,
                            position.to_array(),
                            yaw_degrees,
                            scale,
                        ) {
                            Ok(()) => spawned_character = true,
                            Err(reason) => {
                                crate::engine::logging::info(format_args!(
                                    "[characters] `{model}` spawns through the dynamic path: \
                                     {reason}"
                                ));
                            }
                        }
                    }
                    let shadow_only = self
                        .game
                        .entities()
                        .handle_for_dynamic_key(entity)
                        .and_then(|handle| {
                            self.game.entities().components().renderables.get(handle)
                        })
                        .is_some_and(|renderable| renderable.shadow_only);
                    if !spawned_character
                        && (shadow_only
                            || !self.renderer.spawn_runtime_model(
                                entity,
                                &model,
                                position.to_array(),
                                yaw_degrees,
                                scale,
                            ))
                    {
                        self.game.entities_mut().note_spawn_failure();
                        crate::engine::logging::warn_once(
                            format!("runtime-spawn:{model}"),
                            format!(
                                "[entities] runtime spawn of `{model}` failed; the entity exists \
                                 but draws nothing"
                            ),
                        );
                    }
                }
                crate::engine::entities::WorldCommand::DespawnDynamic {
                    entity,
                    instance_id,
                } => {
                    let _despawn_runtime_model_status = self.renderer.despawn_runtime_model(entity);
                    if !instance_id.is_empty() {
                        let _ragdoll_removed = self.items.world.remove_ragdoll(&instance_id);
                        let _despawn_runtime_character_status =
                            self.renderer.despawn_runtime_character(&instance_id);
                    }
                }
                crate::engine::entities::WorldCommand::SetDynamicTransform {
                    entity,
                    position,
                    yaw_degrees,
                } => {
                    let _runtime_transform_changed = self.renderer.set_runtime_transform(
                        entity,
                        position.to_array(),
                        yaw_degrees,
                    );
                    if let Some(instance_id) = self
                        .game
                        .entities()
                        .handle_for_dynamic_key(entity)
                        .and_then(|handle| self.game.entities().instance_id_of(handle))
                    {
                        let _runtime_character_transform_changed =
                            self.renderer.set_runtime_character_transform(
                                instance_id,
                                position.to_array(),
                                yaw_degrees,
                            );
                    }
                }
                crate::engine::entities::WorldCommand::SetDynamicEmission { entity, scale } => {
                    let _runtime_emission_changed =
                        self.renderer.set_runtime_emission(entity, scale);
                }
                crate::engine::entities::WorldCommand::PlaySound {
                    entity,
                    sound,
                    gain,
                    looped,
                    time_domain,
                } => {
                    self.play_authored_sound(entity, &sound, gain, looped, time_domain);
                }
                crate::engine::entities::WorldCommand::StopSound { entity } => {
                    self.audio
                        .system
                        .stop_source(crate::engine::audio::SourceId(entity));
                }
                crate::engine::entities::WorldCommand::SetEffectEnabled { index, enabled } => {
                    let effect_index = usize::try_from(index).unwrap_or(usize::MAX);
                    let _effect_enabled_changed =
                        self.renderer.set_effect_enabled(effect_index, enabled);
                }
                crate::engine::entities::WorldCommand::SetWaterEnabled { index, enabled } => {
                    let water_index = usize::try_from(index).unwrap_or(usize::MAX);
                    self.game.set_water_enabled(water_index, enabled);
                }
            }
        }
    }

    /// Stops the frame loop when the active backend hit a fatal GPU error.
    ///
    /// The backend reported the error itself (once); this only ends the
    /// process through the ordinary shutdown path and keeps the message for
    /// `main` to exit with.
    fn stop_on_fatal_renderer_error(&mut self) -> bool {
        let Some(error) = self.renderer.fatal_error().map(str::to_string) else {
            return false;
        };
        self.fatal_error = Some(error);
        self.game.stop();
        true
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
        if apply.window
            && let Err(error) = self.apply_window_settings()
        {
            self.ui_state
                .set_status(format!("Could not change the window: {error}"), true);
        }
        if apply.audio {
            self.renderer.set_visual_style(
                self.settings.visual_style.shader_value(),
                self.ready_seconds,
            );
            self.update_audio_mix();
        }
        if apply.vsync {
            let interval = self
                .renderer
                .set_swap_interval(self.settings.vsync_enabled());
            self.bench.set_reported_swap_interval(interval);
        }
        if apply.graphics {
            // An explicit graphics change is a fresh request: a previous
            // failure of the same configuration must not hold it back, and a
            // re-selection repairs a renderer that holds an older one.
            self.failed_graphics = None;
            self.schedule_graphics_transition();
        }
        // State-driven: even with nothing pending, the requested configuration
        // may differ from what the renderer holds (a commit that delivered an
        // older configuration, a cancelled transition, a failed renderer
        // apply). This schedules the outstanding work and is a handful of
        // comparisons when there is none.
        self.reconcile_graphics();
    }

    /// The application scene owns its circuit presentation independently of
    /// the saved gameplay preference. Never rewrite settings to load the menu.
    fn graphics_for_level(&self, id: &str) -> quality::GraphicsSpec {
        let mut spec = self.settings.graphics_spec();
        if id == "places_menu" {
            spec.lightmaps = quality::LightmapQuality::Full;
        }
        spec
    }

    fn requested_graphics(&self) -> quality::GraphicsSpec {
        let id = self
            .load_source
            .as_ref()
            .map(loading::Source::level_id)
            .or_else(|| {
                self.current_level
                    .as_ref()
                    .map(|loaded| loaded.level.id.as_str())
            });
        self.graphics_for_level(id.unwrap_or_default())
    }

    /// Reconciles the requested graphics configuration against the renderer.
    ///
    /// Runs every frame: a graphics transition is a non-blocking background
    /// job, so the game loop is the one place that can guarantee the last
    /// selection eventually wins. The decision itself is
    /// [`quality::graphics_action`]; this only performs it on the live
    /// systems.
    fn reconcile_graphics(&mut self) {
        let action = quality::graphics_action(
            self.requested_graphics(),
            self.renderer.graphics_applied(),
            self.renderer.staged_graphics(),
            self.failed_graphics,
            self.load_intent.is_some(),
        );
        match action {
            quality::GraphicsAction::Settled => self.failed_graphics = None,
            // An outstanding install of a superseded configuration would
            // overwrite the configuration the player asked for again.
            quality::GraphicsAction::CancelStale => {
                if matches!(self.load_intent, Some(LoadIntent::Graphics)) {
                    self.cancel_graphics_transition();
                }
            }
            quality::GraphicsAction::Schedule => {
                if self
                    .load_intent
                    .is_none_or(|intent| matches!(intent, LoadIntent::Graphics))
                {
                    self.schedule_graphics_transition();
                }
            }
            quality::GraphicsAction::AwaitCommit | quality::GraphicsAction::Held => {}
        }
    }

    /// Abandons an outstanding graphics transition, keeping the selection.
    ///
    /// Used when a newer selection makes the outstanding install stale. The
    /// settings are untouched (the player asked for them), the renderer is
    /// left holding the resident configuration, and the state-driven
    /// reconciliation picks the remaining transition up on a later frame.
    fn cancel_graphics_transition(&mut self) {
        self.loader.cancel();
        self.renderer.abort_graphics_request();
        self.pending_commit = None;
        self.load_intent = None;
        self.load_source = None;
        self.load_phase = None;
        self.game.set_app_state(self.load_return_state);
        self.ui_state.clear_status();
        self.trace
            .record("graphics_cancelled", self.load_generation, "");
    }

    /// Applies every changed graphics setting as one transaction, or schedules
    /// the preparation it needs.
    ///
    /// The renderer records the requested configuration; this hands over the
    /// level already resident and lets the renderer diff it against what is
    /// applied. Filtering and Bloom cost no resource work, a quality change
    /// re-fits the retained build, a Reflections change retires or creates the
    /// probe/planar resources, and a Lightmaps change rebuilds the CPU level
    /// once and fills an uncached atlas on a worker while the previous world
    /// keeps rendering. The game world — player position, camera, pause state,
    /// the `Game` struct — is not touched: this is a renderer reconfiguration,
    /// not a level load.
    fn schedule_graphics_transition(&mut self) {
        // Any explicit schedule is a fresh attempt at the current request.
        self.failed_graphics = None;
        self.request_renderer_graphics();
        self.schedule_requested_graphics();
    }

    /// Publishes the selected configuration after a failed request may have
    /// restored the renderer's request to its previous resident configuration.
    fn request_renderer_graphics(&mut self) {
        let lightmaps = self.requested_graphics().lightmaps;
        self.renderer.set_quality(self.settings.quality_level());
        self.renderer
            .set_lighting_quality(self.settings.lighting_quality());
        self.renderer.set_lightmap_quality(lightmaps);
        self.renderer
            .set_reflection_quality(self.settings.reflection_quality());
        self.renderer
            .set_bloom_enabled(self.settings.bloom_enabled());
        self.renderer
            .set_texture_filtering(self.settings.texture_filtering_preset());
    }

    fn schedule_requested_graphics(&mut self) {
        if self.load_intent.is_none() && self.renderer.apply_frame_graphics() {
            self.trace.record(
                "settings_applied",
                self.load_generation,
                &serde_json::json!({
                    "quality": self.settings.quality_level().name(),
                    "lighting": self.settings.lighting_quality().name(),
                    "use_low_quality_lighting": self.settings.use_low_quality_lighting,
                    "lightmaps": self.requested_graphics().lightmaps.name(),
                    "reflections": self.settings.reflection_quality().name(),
                    "bloom": self.settings.bloom_enabled(),
                    "filtering": self.settings.texture_filtering_preset(),
                })
                .to_string(),
            );
            return;
        }
        let source = self.load_source.clone().or_else(|| {
            self.current_level
                .as_ref()
                .map(|level| loading::Source::retained(level.clone()))
        });
        let Some(rebuild_source) = source else {
            // Nothing to rebuild from: the request cannot be scheduled here.
            // Hold it so the reconciliation does not retry every frame; the
            // next world commit makes the request schedulable again.
            self.failed_graphics = Some(self.requested_graphics());
            return;
        };
        let intent = self.load_intent.unwrap_or(LoadIntent::Graphics);
        // The transition starts here: the harness measures settings changes
        // from this mark to the completion mark (the world commit).
        self.trace.record(
            "settings_change",
            self.load_generation,
            &serde_json::json!({
                "quality": self.settings.quality_level().name(),
                "lighting": self.settings.lighting_quality().name(),
                "use_low_quality_lighting": self.settings.use_low_quality_lighting,
                "lightmaps": self.requested_graphics().lightmaps.name(),
                "reflections": self.settings.reflection_quality().name(),
                "bloom": self.settings.bloom_enabled(),
                "filtering": self.settings.texture_filtering_preset(),
            })
            .to_string(),
        );
        self.request_load(rebuild_source, intent);
    }

    /// Keeps the one-line "Applying ..." hint in step with the renderer.
    ///
    /// An uncached lightmap quality change fills on a worker while the previous
    /// world keeps rendering; the game loop stays interactive, so a subtle
    /// status line is the only feedback the settings screen needs. The hint is
    /// shown only when nothing else is being reported, and it is cleared only
    /// when it is the message still on screen, so a genuine status (a saved
    /// binding, a load failure) is never overwritten or dropped.
    fn refresh_graphics_status_hint(&mut self) {
        match self.renderer.graphics_transition_status() {
            GraphicsTransition::Preparing(stage) => {
                if self.ui_state.status_message.is_none() {
                    let hint = format!("Applying {stage}...");
                    self.ui_state.set_status(hint.clone(), false);
                    self.graphics_hint = Some(hint);
                }
            }
            GraphicsTransition::Idle => {
                // Clear exactly the hint this function installed, even if a
                // screen kept it on show across frames; a genuine status that
                // replaced it stays.
                if self.graphics_hint.is_some()
                    && self.graphics_hint.as_deref() == self.ui_state.status_message.as_deref()
                {
                    self.ui_state.clear_status();
                }
                self.graphics_hint = None;
            }
        }
    }

    /// Applies one scripted `PLACES_BENCH_GRAPHICS_CYCLE` change.
    ///
    /// Each entry goes through the same [`Settings`] setter the menu uses, so a
    /// direct advanced change is an override: it never cascades the overall
    /// quality preset, exactly as a menu change to that row would not.
    fn apply_bench_graphics_change(&mut self, change: bench::GraphicsChange) {
        match change {
            bench::GraphicsChange::Filtering(name) => {
                let _ = self.settings.set_texture_filtering(name);
            }
            bench::GraphicsChange::Lightmaps(quality) => {
                let _ = self.settings.set_lightmap_quality(quality);
            }
            bench::GraphicsChange::Reflections(quality) => {
                let _ = self.settings.set_reflection_quality(quality);
            }
            bench::GraphicsChange::Bloom(enabled) => {
                let _ = self.settings.set_bloom(enabled);
            }
            bench::GraphicsChange::LowLighting(enabled) => {
                let _ = self.settings.set_use_low_quality_lighting(enabled);
            }
        }
    }

    fn apply_bench_lighting_actions(&mut self) {
        if self.load_intent.is_some() || self.current_level.is_none() {
            return;
        }
        for action in self
            .bench
            .lighting_actions_at(self.ready_frames.saturating_add(1))
        {
            let applied = match &action {
                bench::lighting_sequence::LightingAction::Camera {
                    position,
                    yaw,
                    pitch,
                } => self.bench.set_camera_override(*position, *yaw, *pitch),
                bench::lighting_sequence::LightingAction::Player {
                    position,
                    yaw,
                    pitch,
                } => {
                    self.game.set_benchmark_player_position(
                        Vec3::from_array(*position),
                        yaw.to_radians(),
                        pitch.to_radians(),
                    );
                    true
                }
                bench::lighting_sequence::LightingAction::Spawn {
                    key,
                    model,
                    character,
                    position,
                    yaw,
                    scale,
                } => {
                    if *character {
                        self.renderer
                            .spawn_runtime_character(
                                &format!("bench#{key}"),
                                model,
                                *position,
                                *yaw,
                                *scale,
                            )
                            .is_ok()
                    } else {
                        self.renderer
                            .spawn_runtime_model(*key, model, *position, *yaw, *scale)
                    }
                }
                bench::lighting_sequence::LightingAction::Move { key, position, yaw } => {
                    self.renderer.set_runtime_transform(*key, *position, *yaw)
                        || self.renderer.set_runtime_character_transform(
                            &format!("bench#{key}"),
                            *position,
                            *yaw,
                        )
                }
                bench::lighting_sequence::LightingAction::Despawn { key } => {
                    self.renderer.despawn_runtime_model(*key)
                        || self
                            .renderer
                            .despawn_runtime_character(&format!("bench#{key}"))
                }
                bench::lighting_sequence::LightingAction::ToggleFixture { fixture, enabled } => {
                    self.apply_bench_fixture(*fixture, *enabled)
                }
                bench::lighting_sequence::LightingAction::Door { id, open } => {
                    let target = Some(id.clone());
                    let door_action = if *open {
                        level::ActionDef::Open { target }
                    } else {
                        level::ActionDef::Close { target }
                    };
                    let report = self.game.dispatch_actions(&[door_action], None);
                    report.unsupported == 0
                        && report.missing_targets == 0
                        && report.doors_acted == 1
                }
            };
            crate::engine::logging::info(format!(
                "[lighting-sequence] frame={} action={action:?} applied={applied}",
                self.ready_frames.saturating_add(1)
            ));
            if !applied {
                self.fatal_error = Some("Native lighting sequence action failed".to_string());
                self.game.stop();
            }
        }
    }

    fn apply_bench_fixture(&mut self, fixture: usize, enabled: bool) -> bool {
        let Some(id) = self
            .current_level
            .as_ref()
            .and_then(|loaded| loaded.level.ceiling_lights.get(fixture))
            .and_then(|light| light.id.clone())
        else {
            return false;
        };
        let report = self.game.dispatch_actions(
            &[level::ActionDef::SetLight {
                target: Some(id),
                on: enabled,
            }],
            None,
        );
        let toggles = self.game.take_light_toggles();
        self.renderer.apply_light_toggles(&toggles);
        report.unsupported == 0 && report.missing_targets == 0 && report.lights_toggled == 1
    }

    /// Performs one scripted window action from `PLACES_BENCH_WINDOW_CYCLE`.
    ///
    /// The action is a real SDL window operation — `set_size`, `minimize` or
    /// `restore` — so the platform emits its own resize/iconify events and the
    /// normal frame loop observes the resulting drawable size. Errors are
    /// ignored like every other benchmark override: the run continues and the
    /// resulting drawable is whatever the window now reports.
    fn apply_window_action(&mut self, action: bench::WindowAction) {
        let result: Result<(), String> = match action {
            bench::WindowAction::Resize(width, height) => self
                .window
                .set_size(width, height)
                .map_err(|error| error.to_string()),
            bench::WindowAction::Minimize => {
                if self.window.minimize() {
                    Ok(())
                } else {
                    Err(sdl3::get_error().to_string())
                }
            }
            bench::WindowAction::Restore => {
                if self.window.restore() {
                    Ok(())
                } else {
                    Err(sdl3::get_error().to_string())
                }
            }
            bench::WindowAction::Raise => {
                if self.window.raise() {
                    Ok(())
                } else {
                    Err(sdl3::get_error().to_string())
                }
            }
        };
        if let Err(error) = result {
            crate::engine::logging::warn_once(
                "bench-window-cycle",
                format!("PLACES_BENCH_WINDOW_CYCLE: window action failed: {error}"),
            );
        }
    }

    /// Applies the window mode and size to the live window.
    ///
    /// A windowed size that does not fit the work area is reduced to fit and
    /// adopted back into the settings, so restoring from fullscreen always
    /// lands on a usable window and the menu shows the real one.
    fn apply_window_settings(&mut self) -> Result<(), String> {
        if self.settings.window_mode() == WindowMode::Fullscreen {
            // SDL3 has one fullscreen boolean; with no display mode set it is
            // borderless desktop fullscreen, the mode Places uses.
            return self
                .window
                .set_fullscreen(true)
                .map_err(|error| error.to_string());
        }
        self.window
            .set_fullscreen(false)
            .map_err(|error| error.to_string())?;
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

    /// Refreshes the actual window/display state the Display screen reports.
    ///
    /// The backend is the source of truth: a fullscreen request the platform
    /// refused, a manual window resize and a monitor or backing-scale change all
    /// show up here (and a manual resize is adopted into the settings, so the
    /// menu and the persisted configuration follow the real window).
    fn refresh_display_status(&mut self) {
        let mode = match self.window.fullscreen_state() {
            FullscreenType::Off => WindowMode::Windowed,
            // SDL3 has no separate desktop-fullscreen flag: a borderless
            // desktop fullscreen window reports `True`, while the crate's
            // `Desktop` variant tests a flag bit SDL3 uses for
            // `SDL_WINDOW_MODAL`, so it is never produced. Places only ever
            // requests borderless desktop fullscreen, so `Off` vs not-`Off` is
            // exact.
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
        let display = window_display(self.window, self.video_subsystem);
        let usable_bounds = display.map_or((0, 0), usable_display_bounds);
        let desktop_size = display.map_or((0, 0), display_bounds);
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
        let (drawable_width, drawable_height) = self.window.size_in_pixels();
        let logged = (
            window_size.0,
            window_size.1,
            drawable_width,
            drawable_height,
        );
        if logged != self.last_logged_window {
            self.last_logged_window = logged;
            self.trace.record(
                "window_size_observed",
                self.load_generation,
                &serde_json::json!({"logical":[window_size.0,window_size.1],
                    "drawable":[drawable_width,drawable_height]})
                .to_string(),
            );
            logging::info(format!(
                "[window] logical {}x{} | drawable {}x{} pixels | backing scale {:.1}x{}",
                window_size.0,
                window_size.1,
                drawable_width,
                drawable_height,
                if window_size.0 == 0 {
                    0.0_f64
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

    /// `PLACES_STATE_LOG=file.csv`: append the player state every few frames.
    ///
    /// Development diagnostics for control validation on a real machine; it
    /// never changes gameplay and is inert unless the variable is set.
    fn log_player_state(&mut self) {
        if let Some(log) = self.state_log.as_mut()
            && self.game.frame_count().is_multiple_of(STATE_LOG_INTERVAL)
            && let Err(error) = writeln!(
                log,
                "{},{:.4},{:.4},{:.4},{:.4},{:.4}",
                self.game.frame_count(),
                self.game.player_position.x,
                self.game.player_position.y,
                self.game.player_position.z,
                self.game.player_yaw,
                self.game.player_pitch
            )
        {
            logging::warn(format!("[state] could not write player state: {error}"));
            *self.state_log = None;
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

    /// Reconciles SDL's relative mouse mode with the game state.
    ///
    /// Relative mode is on exactly while gameplay is active and the window has
    /// focus: every menu, the pause screen and a focus loss release the cursor,
    /// and regaining focus while Playing captures it again. The cursor is never
    /// warped. SDL is polled for the real state rather than caching what the
    /// last call asked for: a platform can drop relative mode on its own (a
    /// focus change, an OS capture change), and the mode is re-asserted on the
    /// next frame whenever it differs from the desired one.
    fn sync_mouse_capture(&mut self) {
        let desired = !self.console.is_open()
            && self.load_intent.is_none()
            && self.game.app_state() == AppState::Playing
            && self.window_focused;
        let captured = self.sdl.mouse().relative_mouse_mode(self.window);
        if !desired || !captured {
            self.items.input.invalidate();
        }
        if captured != desired {
            self.sdl
                .mouse()
                .set_relative_mouse_mode(self.window, desired);
        }
        let text_input = self.video_subsystem.text_input();
        let wants_text = self.console.is_open() && self.window_focused;
        if text_input.is_active(self.window) != wants_text {
            if wants_text {
                text_input.start(self.window);
            } else {
                text_input.stop(self.window);
            }
        }
        if self.sdl.mouse().is_cursor_showing() == desired {
            self.sdl.mouse().show_cursor(!desired);
        }
    }

    fn sync_menu_pointer(&mut self) {
        self.ui_state.loading_map = self.startup == StartupPhase::Ready
            && matches!(
                self.load_intent,
                Some(LoadIntent::Play | LoadIntent::Background)
            );
        self.ui_state.keep_selection_visible(self.game.app_state());
        self.ui_state.sync_pointer_context(
            self.game.app_state(),
            self.window.size(),
            self.window.size_in_pixels(),
            self.window_focused && self.startup == StartupPhase::Ready,
        );
    }

    /// All menu clicks use the rendered layout and the renderer's actual
    /// viewport conversion. No desktop coordinates or hard-coded DPI factor.
    fn menu_hit(&self, x: f32, y: f32) -> Option<ui::MenuHit> {
        if !self.window_focused
            || self.startup != StartupPhase::Ready
            || self.game.app_state() == AppState::Playing
        {
            return None;
        }
        let (width, height) = self.window.size_in_pixels();
        let point = DrawableSize::new(width, height).window_to_ui(self.window.size(), [x, y])?;
        ui::menu_layout(
            self.game.app_state(),
            self.ui_state,
            self.settings,
            self.display_status,
        )
        .hit_test(point)
    }

    fn handle_menu_pointer(&mut self, event: &Event) -> bool {
        self.sync_menu_pointer();
        if let Event::MouseButtonDown {
            mouse_btn: MouseButton::Left,
            x,
            y,
            timestamp: _,
            window_id: _,
            which: _,
            clicks: _,
        }
        | Event::MouseButtonUp {
            mouse_btn: MouseButton::Left,
            x,
            y,
            timestamp: _,
            window_id: _,
            which: _,
            clicks: _,
        } = event
        {
            let hit = self.menu_hit(*x, *y);
            let down = matches!(
                event,
                Event::MouseButtonDown {
                    timestamp: _,
                    window_id: _,
                    which: _,
                    mouse_btn: _,
                    clicks: _,
                    x: _,
                    y: _
                }
            );
            if !down {
                self.ui_state.volume_drag = None;
            }
            if let Some(clicked) = self.ui_state.pointer.button(down, hit) {
                self.activate_menu_hit(clicked);
                self.sync_menu_pointer();
                self.sync_mouse_capture();
            }
            return true;
        }
        if self.game.app_state() == AppState::Playing {
            return false;
        }
        if self.handle_menu_motion(event) {
            return true;
        }
        if let Event::MouseWheel {
            y,
            direction,
            mouse_x,
            mouse_y,
            timestamp: _,
            window_id: _,
            which: _,
            x: _,
            integer_x: _,
            integer_y: _,
        } = event
        {
            if self.window_focused
                && !self.ui_state.loading_map
                && self.ui_state.rebinding_action.is_none()
                && self.menu_hit(*mouse_x, *mouse_y).is_some()
                && y.is_finite()
                && *y != 0.0
            {
                let flipped = *direction == MouseWheelDirection::Flipped;
                let step = if (*y > 0.0) == flipped { 1_i32 } else { -1_i32 };
                if self
                    .menu_hit(*mouse_x, *mouse_y)
                    .is_some_and(|hit| matches!(hit.intent, ui::MenuIntent::Page(_)))
                {
                    self.ui_state.scroll_details(step);
                } else {
                    self.ui_state.scroll_menu(self.game.app_state(), step);
                }
            }
            return true;
        }
        false
    }

    fn handle_menu_motion(&mut self, event: &Event) -> bool {
        if let Event::MouseMotion {
            x,
            y,
            xrel,
            yrel,
            timestamp: _,
            window_id: _,
            which: _,
            mousestate: _,
        } = event
        {
            // SDL emits stationary/synthetic events when capture changes. They
            // must not replace keyboard selection or hover on menu opening.
            if self.window_focused && (xrel.abs() > 0.0 || yrel.abs() > 0.0) {
                let hit = self.menu_hit(*x, *y);
                if self.ui_state.volume_drag.is_some()
                    && let Some(slider) = hit
                    && matches!(slider.intent, ui::MenuIntent::Volume(_))
                {
                    self.activate_menu_hit(slider);
                    return true;
                }
                if self.ui_state.pointer.motion([*x, *y], hit)
                    && let Some(hovered) = hit
                    && matches!(
                        hovered.intent,
                        ui::MenuIntent::Activate | ui::MenuIntent::Adjust(_)
                    )
                {
                    self.ui_state
                        .select_menu_index(self.game.app_state(), hovered.index);
                }
            }
            return true;
        }
        false
    }

    fn activate_menu_hit(&mut self, clicked: ui::MenuHit) {
        match clicked.intent {
            ui::MenuIntent::Dismiss => {
                self.ui_state.clear_status();
                self.ui_state.credits = None;
            }
            ui::MenuIntent::Page(direction) => self.ui_state.scroll_details(direction),
            ui::MenuIntent::Volume(percent) => {
                self.ui_state.volume_drag = Some(());
                let _changed = self.settings.set_master_volume(f32::from(percent) / 100.0);
                self.save_settings();
            }
            ui::MenuIntent::CancelRebind => self.ui_state.cancel_rebinding(),
            ui::MenuIntent::CancelLoad => self.cancel_loading(),
            ui::MenuIntent::Scroll(direction) => {
                self.ui_state.scroll_menu(self.game.app_state(), direction);
            }
            ui::MenuIntent::Activate | ui::MenuIntent::Adjust(_) => {
                self.ui_state
                    .select_menu_index(self.game.app_state(), clicked.index);
                if let ui::MenuIntent::Adjust(direction) = clicked.intent {
                    self.adjust_settings(direction);
                } else {
                    self.activate();
                }
            }
        }
    }

    fn handle_scripted_action(&mut self, received: actions::Received) -> bool {
        self.trace_scripted_action(&received);
        self.apply_item_script_action(&received.action);
        self.apply_scripted_caustics(&received.action);
        self.apply_scripted_navigation(&received.action);
        match received.action {
            actions::Action::Load { level } => self.scripted_load_level(&level),
            actions::Action::Escape {} => {
                self.scripted_key("Escape", true);
                self.scripted_key("Escape", false);
            }
            actions::Action::ConsoleEnabled { enabled } => self.set_scripted_console(enabled),
            actions::Action::Key {
                key,
                pressed,
                repeat,
            } => self.scripted_key_event(&key, pressed, repeat),
            actions::Action::AimEntity {
                id,
                yaw_offset_degrees,
            } => self.scripted_aim_entity(&id, yaw_offset_degrees),
            actions::Action::View { yaw, pitch } => self.scripted_view(yaw, pitch),
            actions::Action::ViewAt { position } => self.scripted_view_at(position),
            actions::Action::Text { text, submit } => {
                self.scripted_text(text, submit);
            }
            actions::Action::NavigationGoal { id: _, position: _ }
            | actions::Action::Player {
                position: _,
                yaw: _,
                pitch: _,
            }
            | actions::Action::ItemPose {
                id: _,
                position: _,
                rotation: _,
            }
            | actions::Action::ItemVelocity {
                id: _,
                linear: _,
                angular: _,
            }
            | actions::Action::CausticsSource {
                id: _,
                on: _,
                intensity: _,
                color: _,
            }
            | actions::Action::CausticsWater {
                index: _,
                tint: _,
                speed: _,
                strength: _,
            }
            | actions::Action::Fixture { id: _, on: _ }
            | actions::Action::AimItem { id: _ }
            | actions::Action::Activate { id: _ }
            | actions::Action::Mouse {
                button: _,
                pressed: _,
                recapture: _,
            } => {}
            actions::Action::Capture { name } => {
                self.items.captures.push_back(name);
            }
            actions::Action::Resize { width, height } => {
                self.resize_scripted_window(width, height);
            }
            actions::Action::Quality { level } => {
                if let Some(quality) = quality::QualityLevel::parse(&level)
                    && !self.settings.set_quality(quality)
                {
                    // The script asked for the configuration already stored:
                    // still an explicit request, so it may repair a renderer
                    // that holds an older one.
                    self.settings.request_graphics_reapply();
                }
            }
            actions::Action::Lightmaps { quality } => {
                if let Some(lightmap_quality) = quality::LightmapQuality::parse(&quality) {
                    let _lightmap_quality_changed =
                        self.settings.set_lightmap_quality(lightmap_quality);
                }
            }
            actions::Action::LowLighting { enabled } => {
                if !self.settings.set_use_low_quality_lighting(enabled) {
                    self.settings.request_graphics_reapply();
                }
            }
            actions::Action::Focus { focused } => return self.scripted_focus(focused),
            actions::Action::Quit {} => {
                if !self.items.captures.is_empty() {
                    self.items.quit_after_captures = true;
                    return true;
                }
                return self.handle_event(&Event::Quit { timestamp: 0 });
            }
        }
        self.trace_world("action_result");
        self.game.is_running()
    }

    fn apply_scripted_navigation(&mut self, action: &actions::Action) {
        if let actions::Action::NavigationGoal { id, position } = action {
            let accepted = self
                .game
                .entities_mut()
                .set_ai_navigation_goal(id, Vec3::from_array(*position));
            if !accepted {
                logging::warn(format!("[navigation] diagnostic goal refused for {id}"));
            }
        }
    }

    fn scripted_load_level(&mut self, level: &str) {
        if let Some(index) = self
            .level_manager
            .entries()
            .iter()
            .position(|entry| entry.id == level)
        {
            self.load_level_at(index);
        } else {
            self.fatal_error = Some(format!("Script requested unknown level {level}"));
            self.game.stop();
        }
    }

    fn apply_scripted_caustics(&mut self, action: &actions::Action) {
        let failure = if let actions::Action::CausticsSource {
            id,
            on,
            intensity,
            color,
        } = action
        {
            (!self
                .renderer
                .set_caustics_source(id, *on, *intensity, *color))
            .then(|| format!("Native caustics source refused for `{id}`"))
        } else if let actions::Action::CausticsWater {
            index,
            tint,
            speed,
            strength,
        } = action
        {
            (!self
                .renderer
                .set_caustics_water(*index, *tint, *speed, *strength))
            .then(|| format!("Native caustics water refused for index {index}"))
        } else {
            None
        };
        if let Some(error) = failure {
            self.fatal_error = Some(error);
            self.game.stop();
        }
    }

    fn scripted_focus(&mut self, focused: bool) -> bool {
        if focused {
            let raised = self.window.raise();
            self.trace.record("native_window_raise", self.load_generation, &serde_json::json!({"requested":raised,"observed_focus":self.window.has_input_focus()}).to_string());
        }
        self.handle_event(&Event::Window {
            timestamp: 0,
            window_id: self.window.id(),
            win_event: if focused {
                WindowEvent::FocusGained
            } else {
                WindowEvent::FocusLost
            },
        })
    }

    fn trace_scripted_action(&mut self, received: &actions::Received) {
        self.trace.record(
            "action",
            self.load_generation,
            &serde_json::json!({
                "id": received.id, "kind": format!("{:?}", received.action),
                "latency_ms": received.latency.as_secs_f64() * 1_000.0_f64,
            })
            .to_string(),
        );
    }

    fn handle_graphics_key_release(&mut self, event: &Event) -> bool {
        matches!(self.load_intent, Some(LoadIntent::Graphics))
            && self.load_return_state == AppState::Playing
            && self.game.app_state() == AppState::Paused
            && self.window_focused
            && self
                .input_handler
                .handle_graphics_key_release(event, &self.settings.bindings)
    }

    fn handle_menu_window_event(&mut self, event: &Event) -> bool {
        if let Event::Window {
            win_event,
            timestamp: _,
            window_id: _,
        } = event
        {
            if matches!(win_event, WindowEvent::FocusLost | WindowEvent::FocusGained) {
                self.items
                    .input
                    .focus_changed(self.event_pump.mouse_state().left());
            }
            if matches!(win_event, WindowEvent::FocusLost) {
                self.window_focused = false;
                self.ui_state.volume_drag = None;
                self.game.reset_timing();
                self.close_console();
                // A key released while another window had focus may never
                // deliver its KeyUp. Releasing everything keeps a held Jump,
                // Crouch or Interact from surviving the refocus.
                self.input_handler.clear_gameplay_inputs();
            } else if matches!(win_event, WindowEvent::FocusGained) {
                self.window_focused = true;
                self.game.reset_timing();
                self.ui_state
                    .pointer
                    .focus(self.event_pump.mouse_state().left());
            }
            self.ui_state.pointer.invalidate();
            self.sync_mouse_capture();
            return true;
        }

        false
    }

    /// Handles one SDL event; returns `false` when the pump should stop.
    fn handle_event(&mut self, event: &Event) -> bool {
        self.observe_item_mouse(event);
        if let Some(received) = self
            .actions
            .as_ref()
            .and_then(|actions| actions.receive(event))
        {
            return self.handle_scripted_action(received);
        }

        if let Event::Quit { timestamp: _ } = event {
            self.game.stop();
            return false;
        }

        if self.handle_menu_window_event(event) {
            return true;
        }

        if self.handle_menu_pointer(event) {
            return true;
        }

        if !self.window_focused {
            return true;
        }

        // Until the first world is committed, preparation is the only thing
        // that can produce the menu background, so no menu or cancel input is
        // accepted: window events, Quit and scripted actions were handled
        // above and still work.
        if self.startup != StartupPhase::Ready {
            return true;
        }

        if self.load_intent.is_some()
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    keymod: _,
                    which: _,
                    raw: _
                }
            )
        {
            // A graphics transition owns no screen and blocks nothing, so
            // Escape backs out of whatever screen is showing (or resumes the
            // game) and leaves the transition running; it must never rewrite
            // the player's saved selection with a rolled-back value. A level
            // load is still cancellable.
            if !matches!(self.load_intent, Some(LoadIntent::Graphics)) {
                self.cancel_loading();
                return true;
            }
        }

        // A map load has one Cancel action; keyboard and pointer cannot
        // start another operation behind the loading panel.
        if self.ui_state.loading_map || self.handle_outcome_event(event) {
            return true;
        }

        // Rebinding consumes keyboard input before overlay/menu/gameplay keys.
        if let Some(action) = self.ui_state.rebinding_action {
            self.handle_rebinding_event(event, action);
            return true;
        }

        if self.handle_graphics_key_release(event)
            || self.handle_console_event(event)
            || self.handle_menu_details_key(event)
        {
            return true;
        }

        // Development diagnostics share the normal native capture path and
        // never request a level rebuild or alter the player's settings.
        #[cfg(feature = "visual-diagnostics")]
        if self.handle_visual_diagnostic_event(event) {
            return true;
        }

        // Performance overlay toggle with '-' key (hidden by default, reserved
        // so it can never also be a gameplay binding)
        if let Event::KeyDown {
            keycode: Some(Keycode::Minus | Keycode::KpMinus),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
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
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } = event
            {
                self.input_handler.clear_gameplay_inputs();
                self.game.handle_escape(); // Opens Pause menu
            } else {
                self.input_handler
                    .handle_gameplay_event(event, &self.settings.bindings);
            }
        } else if let Some(nav) = InputHandler::poll_menu_nav_event(event) {
            self.ui_state.pointer.keyboard();
            self.handle_menu_nav(nav);
        }
        self.sync_menu_pointer();
        self.sync_mouse_capture();
        true
    }

    fn handle_menu_details_key(&mut self, event: &Event) -> bool {
        if self.game.app_state() == AppState::LevelSelect
            && matches!(
                event,
                Event::KeyDown {
                    keycode: Some(Keycode::Tab),
                    repeat: false,
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    keymod: _,
                    which: _,
                    raw: _
                }
            )
        {
            let pages = ui::description_pages(&self.ui_state.level_description);
            self.ui_state.description_page = self
                .ui_state
                .description_page
                .saturating_add(1)
                .checked_rem(pages)
                .unwrap_or(0);
            return true;
        }
        false
    }

    fn handle_guidance_event(&mut self, event: &Event) -> bool {
        if self.message.is_none() {
            return false;
        }

        if let Event::KeyDown {
            keycode: Some(key),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
        } = event
        {
            let rows = self
                .message
                .as_ref()
                .map_or(0, |(_, lines)| ui::wrap_guidance(lines).len());
            match ui::guidance_navigation(*key, self.message_page, rows) {
                ui::GuidanceNavigation::Page(page) => {
                    self.message_page = page;
                    return true;
                }
                ui::GuidanceNavigation::Ignored => return true,
                ui::GuidanceNavigation::Dismiss => {}
            }
            self.message = None;
            self.input_handler.clear_gameplay_inputs();
            self.items.input.invalidate();
            self.game.reset_timing();
            self.trace_world("guidance_dismissed");
        }
        true
    }

    fn handle_outcome_event(&mut self, event: &Event) -> bool {
        if let Event::KeyDown {
            keycode: Some(Keycode::F1),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
        } = event
            && self.game.app_state() == AppState::Playing
            && self.outcome.is_none()
            && self.message.is_none()
        {
            self.game.request_hint();
            return true;
        }
        if self.handle_guidance_event(event) {
            return true;
        }
        if self.outcome.is_some() {
            if let Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } = event
            {
                self.outcome = None;
                self.game.reset_to_spawn();
                self.synchronize_physical_player();
                let groups = self
                    .current_level
                    .as_ref()
                    .map(|loaded| {
                        loaded
                            .level
                            .props
                            .iter()
                            .filter_map(|prop| {
                                prop.physics.as_ref().and_then(|p| p.reset_group.clone())
                            })
                            .collect::<std::collections::BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                for group in groups {
                    self.reset_item_group(&group);
                }
                self.flashlight = flashlight::Power::Off;
                self.input_handler.clear_gameplay_inputs();
                self.reconcile_gameplay_reset();
                self.trace_world("run_restarted");
            } else if let Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } = event
            {
                self.outcome = None;
                self.goto(AppState::MainMenu);
            }
            return true;
        }

        false
    }

    /// Consumes the reserved development selector key after rebinding.
    #[cfg(feature = "visual-diagnostics")]
    fn handle_visual_diagnostic_event(&mut self, event: &Event) -> bool {
        if let Event::KeyDown {
            keycode: Some(Keycode::F8),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
        } = event
        {
            self.renderer.cycle_visual_diagnostic();
            true
        } else {
            false
        }
    }

    /// Handles a key while the Controls screen is waiting for a binding.
    fn handle_rebinding_event(&mut self, event: &Event, action: &str) {
        if let Some(key) = input::pressed_key(event) {
            if key == Keycode::Escape {
                self.ui_state.cancel_rebinding();
            } else {
                let key_str = keycode_to_str(key);
                match self.settings.bindings.set_key(action, &key_str) {
                    Ok(()) => {
                        let label = crate::game::settings::action_label(action);
                        self.ui_state
                            .set_status(format!("Bound {label} to [{key_str}]"), false);
                        if let Err(error) = self.settings.save() {
                            self.ui_state
                                .set_status(settings_save_problem(&error), true);
                        }
                    }
                    Err(err) => {
                        self.ui_state.set_status(err, true);
                    }
                }
                self.ui_state.rebinding_action = None;
            }
        }
    }

    /// Dispatches one menu navigation event for the current screen.
    fn handle_menu_nav(&mut self, nav: MenuNavEvent) {
        if self.ui_state.status_is_error {
            match nav {
                MenuNavEvent::Activate | MenuNavEvent::Back => self.ui_state.clear_status(),
                MenuNavEvent::Up | MenuNavEvent::Left => {
                    self.ui_state.status_page = self.ui_state.status_page.saturating_sub(1);
                }
                MenuNavEvent::Down | MenuNavEvent::Right => {
                    self.ui_state.scroll_details(1);
                }
            }
            return;
        }
        if self.ui_state.credits.is_some() {
            self.ui_state.credits = None;
            return;
        }
        match nav {
            MenuNavEvent::Up => {
                self.cycle_selection(true);
                if self.game.app_state() == AppState::LevelSelect
                    && self.ui_state.level_select_idx < self.ui_state.level_entries.len()
                {
                    self.cycle_selection(true);
                }
            }
            MenuNavEvent::Down => {
                self.cycle_selection(false);
                if self.game.app_state() == AppState::LevelSelect
                    && self.ui_state.level_select_idx < self.ui_state.level_entries.len()
                {
                    self.cycle_selection(false);
                }
            }
            MenuNavEvent::Left => {
                if self.game.app_state() == AppState::LevelSelect {
                    self.cycle_selection(true);
                } else {
                    self.adjust_settings(-1);
                }
            }
            MenuNavEvent::Right => {
                if self.game.app_state() == AppState::LevelSelect {
                    self.cycle_selection(false);
                } else {
                    self.adjust_settings(1);
                }
            }
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
                let count = self
                    .ui_state
                    .settings_page
                    .item_count(self.ui_state.advanced_expanded);
                self.ui_state.settings_idx = menu_prev(self.ui_state.settings_idx, count);
            }
            (AppState::Settings | AppState::PauseSettings, false) => {
                let count = self
                    .ui_state
                    .settings_page
                    .item_count(self.ui_state.advanced_expanded);
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
                .set_status(settings_save_problem(&error), true);
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
        let action = activate_settings_row(
            page,
            index,
            self.ui_state,
            self.settings,
            self.display_status,
        );
        self.save_settings();
        match action {
            SettingsAction::Open(next) => {
                self.ui_state.settings_page = next;
                self.ui_state.settings_idx = 0;
                // The Advanced group starts collapsed on every newly opened
                // settings screen; the expansion state is session UI only.
                self.ui_state.advanced_expanded = false;
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
        self.close_console();
        if self.load_intent.is_some() && state == AppState::Playing {
            return;
        }
        if self.load_intent.is_some() && state == AppState::MainMenu {
            self.cancel_loading();
        }
        if matches!(self.load_intent, Some(LoadIntent::Graphics)) {
            self.load_return_state = state;
        }
        if self.game.app_state() != state {
            self.ui_state.clear_status();
        }
        if state == AppState::MainMenu
            && matches!(
                self.game.app_state(),
                AppState::Playing | AppState::Paused | AppState::PauseSettings
            )
        {
            self.unload_world_for_menu();
            match menu_entry() {
                Ok(entry) => {
                    self.request_load(loading::Source::entry(entry), LoadIntent::Background);
                }
                Err(error) => self.ui_state.set_status(error, true),
            }
        }
        self.game.set_app_state(state);
    }

    fn track_retired_world(&mut self) {
        if !self.bench.enabled()
            || std::env::var("PLACES_BENCH_RESOURCE_TRACE").as_deref() != Ok("1")
        {
            return;
        }
        if let (Some(loaded), Some(lease)) = (self.current_level.as_ref(), &self.displayed_lease) {
            if self.retired_worlds.len() == 32 {
                drop(self.retired_worlds.pop_front());
            }
            self.retired_worlds
                .push_back((loaded.level.id.clone(), lease.lifetime_probe()));
        }
    }

    fn unload_world_for_menu(&mut self) {
        self.startup = StartupPhase::Preparing;
        self.track_retired_world();
        self.loader.cancel();
        self.renderer.cancel_prepared_install();
        self.pending_commit = None;
        self.load_source = None;
        self.load_intent = None;
        self.load_phase = None;
        self.failed_graphics = None;
        self.presentation.pending_scene = None;
        self.ui_state.loading_map = false;
        self.ui_state.current_map = None;
        self.audio.clear_world();
        self.console_runtime = console::runtime::Runtime::default();
        self.unload_items();
        self.game
            .reset_level(Vec3::ZERO, 0.0, game::CollisionWorld::default());
        self.renderer.unload_level();
        *self.current_level = None;
        self.displayed_lease = None;
        self.flashlight = flashlight::Power::Off;
        self.outcome = None;
        self.message = None;
        self.ready_frames = 0;
        self.ready_seconds = 0.0;
        self.input_handler.clear_gameplay_inputs();
        if let Err(error) = self.loader.release_retained() {
            logging::warn(error);
        }
        self.trace_world("world_unloaded");
    }

    /// Main menu: open the level list, open settings, or quit.
    fn activate_main_menu(&mut self) {
        match self.ui_state.main_menu_idx {
            0 => {
                // Discover newly installed external packages when Explore opens.
                self.level_manager.refresh();
                self.refresh_level_entries();
                self.goto(AppState::LevelSelect);
            }
            1 => {
                self.ui_state.settings_page = SettingsPage::Root;
                self.ui_state.settings_idx = 0;
                // Every newly opened settings screen starts with the Advanced
                // group collapsed; the state is never persisted.
                self.ui_state.advanced_expanded = false;
                self.goto(AppState::Settings);
            }
            2 => self.ui_state.credits = Some(()),
            3 => self.game.stop(),
            _ => {}
        }
    }

    /// Refreshes the cached level names from the manager.
    fn refresh_level_entries(&mut self) {
        self.ui_state.description_index = None;
        self.ui_state.level_entries = level_entry_names(self.level_manager);
        self.ui_state.level_select_idx = self
            .ui_state
            .level_select_idx
            .min(self.ui_state.level_entries.len().saturating_add(2));
        self.ui_state.keep_selection_visible(AppState::LevelSelect);
    }

    /// Level list: load the selected level, import packs, or go back.
    fn activate_level_select(&mut self) {
        let num_levels = self.level_manager.menu_entries().len();
        let selected = self.ui_state.level_select_idx;
        match selected.cmp(&num_levels) {
            // One of the installed levels: load it.
            Ordering::Less => {
                if let Some(entry) = self.level_manager.get_menu_entry(selected).cloned() {
                    self.request_load(loading::Source::entry(entry), LoadIntent::Play);
                }
            }
            // `Import Levels`.
            Ordering::Equal => self.import_levels(),
            Ordering::Greater if selected == num_levels.saturating_add(1) => {
                if let Err(error) = open_import_folder() {
                    self.ui_state.set_status(error, true);
                }
            }
            Ordering::Greater => self.goto(AppState::MainMenu),
        }
    }

    /// Queues a level without blocking event dispatch.
    fn load_level_at(&mut self, index: usize) {
        if let Some(entry) = self.level_manager.get_entry(index).cloned() {
            self.request_load(loading::Source::entry(entry), LoadIntent::Play);
        }
    }

    fn request_load(&mut self, source: loading::Source, intent: LoadIntent) {
        self.close_console();
        // An outstanding preparation for the same world and effective lightmap
        // quality is promoted instead of restarted: the startup background
        // world must survive the player selecting that same level, and a
        // partially uploaded install must not be torn down to redo identical
        // work. A superseded request for different content still replaces it.
        if self.reuse_outstanding_preparation(&source, intent) {
            return;
        }
        let level_id = source.level_id().to_string();
        let (entry, identity) = match &source {
            loading::Source::Entry(entry, identity) => (entry, identity),
            loading::Source::Retained(loaded, identity) => (&loaded.entry, identity),
        };
        diagnostics::record(
            "map_request",
            format_args!(
                "level={level_id} intent={intent:?} path={} identity={identity:?}",
                entry.path.display()
            ),
        );
        let lightmaps = self.graphics_for_level(&level_id).lightmaps;
        let request = loading::Request {
            source: source.clone(),
            lightmaps,
        };
        match self.loader.request(request) {
            Ok(id) => {
                self.renderer.set_lightmap_quality(lightmaps);
                if self.load_intent.is_none() {
                    self.load_return_state = if matches!(intent, LoadIntent::Background) {
                        AppState::MainMenu
                    } else {
                        self.game.app_state()
                    };
                }
                self.renderer.cancel_prepared_install();
                self.pending_commit = None;
                self.load_generation = id;
                self.load_intent = Some(intent);
                self.load_source = Some(source);
                self.load_lightmaps = lightmaps;
                self.load_phase = None;
                if self.game.app_state() == AppState::Playing {
                    self.game.set_app_state(AppState::Paused);
                }
                self.clear_loading_inputs(matches!(intent, LoadIntent::Graphics));
                self.game.reset_timing();
                self.ui_state
                    .set_status("Preparing level... Esc to cancel".to_string(), false);
                self.trace.record("request", id, &level_id);
                if let Some(actions) = &self.actions
                    && let Err(error) = actions.notify(&format!("request:{level_id}"))
                {
                    self.fatal_error = Some(error);
                    self.game.stop();
                }
            }
            Err(error) => {
                self.ui_state.set_status(error, true);
                // A request that cannot even be queued must not leave the
                // startup screen gated with nothing being prepared: reveal the
                // recovery screen instead of waiting forever.
                if self.startup != StartupPhase::Ready {
                    self.startup = StartupPhase::Ready;
                    self.game.set_app_state(self.load_return_state);
                }
            }
        }
    }

    /// A live graphics transaction retains only the current Interact hold.
    /// Ordinary loads, menus and unfocused windows still release everything.
    const fn clear_loading_inputs(&mut self, graphics: bool) {
        if graphics && matches!(self.load_return_state, AppState::Playing) && self.window_focused {
            self.input_handler
                .clear_gameplay_inputs_preserving_interact();
        } else {
            self.input_handler.clear_gameplay_inputs();
        }
    }

    /// True when the request already being prepared covers `source` exactly.
    ///
    /// A staged GPU install may only be reused while the configuration it
    /// recorded is still the requested one: the install uploads with the
    /// configuration snapshot it took when staging started, so a selection made
    /// meanwhile must supersede it instead of being coalesced into it. The
    /// lightmap variant is part of that snapshot but is also carried by the
    /// loader request, so it is checked separately while the CPU preparation is
    /// still running.
    fn reuse_outstanding_preparation(
        &mut self,
        source: &loading::Source,
        intent: LoadIntent,
    ) -> bool {
        if self.load_intent.is_none()
            || self.load_lightmaps != self.graphics_for_level(source.level_id()).lightmaps
        {
            return false;
        }
        if self
            .renderer
            .staged_graphics()
            .is_some_and(|staged| staged != self.graphics_for_level(source.level_id()))
        {
            return false;
        }
        let Some(active_intent) = self.load_intent else {
            return false;
        };
        let Some(active) = self.load_source.as_ref() else {
            return false;
        };
        if !active.same_preparation(source) {
            return false;
        }
        let promoted = match (active_intent, intent) {
            (LoadIntent::Play, _) | (_, LoadIntent::Play) => LoadIntent::Play,
            (LoadIntent::Background, _) | (_, LoadIntent::Background) => LoadIntent::Background,
            // Only Graphics intents remain.
            (LoadIntent::Graphics, LoadIntent::Graphics) => LoadIntent::Graphics,
        };
        self.load_intent = Some(promoted);
        if let Some(pending) = self.pending_commit.as_mut() {
            pending.intent = promoted;
        }
        if matches!(promoted, LoadIntent::Play) {
            self.ui_state
                .set_status("Preparing level... Esc to cancel".to_string(), false);
        }
        true
    }

    fn cancel_loading(&mut self) {
        // Before the first world exists, the outstanding preparation is the
        // only thing that can produce a usable background; cancelling it would
        // strand the process on an empty menu with nothing left to wait for.
        if self.startup != StartupPhase::Ready {
            return;
        }
        // A graphics transition is not a level load: it owns no screen, blocks
        // nothing, and cancelling it used to rewrite the player's saved
        // selection with the rolled-back value. Escape backs out of whatever
        // screen is showing and leaves the transition running instead.
        if matches!(self.load_intent, Some(LoadIntent::Graphics)) {
            return;
        }
        self.loader.cancel();
        self.trace
            .record("cancel_disposal_begin", self.load_generation, "");
        self.renderer.cancel_prepared_install();
        self.renderer.abort_graphics_request();
        self.pending_commit = None;
        self.trace
            .record("cancel_disposal_end", self.load_generation, "");
        // The player's graphics selection is never rewritten or saved here: an
        // outstanding change is re-scheduled by `reconcile_graphics` once this
        // cancellation is complete.
        self.load_intent = None;
        self.load_source = None;
        self.load_phase = None;
        self.launch_overrides = false;
        self.input_handler.clear_gameplay_inputs();
        self.game.reset_timing();
        self.game.set_app_state(self.load_return_state);
        self.ui_state.clear_status();
        self.trace.record("cancel", self.load_generation, "");
        self.trace_world("load_cancelled");
    }

    fn advance_loading(&mut self) {
        if let Some((id, result)) = self.loader.poll() {
            match result {
                Ok(prepared) => {
                    let Some(intent) = self.load_intent else {
                        return;
                    };
                    self.trace
                        .record("cpu_ready", id, &prepared.loaded.level.id);
                    if prepared.lightmaps != self.requested_graphics().lightmaps {
                        if let Some(source) = self.load_source.clone() {
                            self.request_load(source, intent);
                        }
                        return;
                    }
                    self.trace.record("preparation_result", id, &serde_json::json!({
                        "level": prepared.loaded.level.id, "wall_ms": prepared.preparation_millis,
                        "cache_hit": prepared.cache_hit,
                        "lighting_ms": if prepared.cache_hit { 0.0_f64 } else { prepared.build.timings.lighting_millis },
                        "props_ms": if prepared.cache_hit { 0.0_f64 } else { prepared.build.timings.props_millis },
                        "surfaces_ms": if prepared.cache_hit { 0.0_f64 } else { prepared.build.timings.surfaces_millis },
                        "atlas_ms": if prepared.cache_hit { 0.0_f64 } else { prepared.build.lightmap_millis },
                        "retained_bytes": prepared.lease.retained_bytes(),
                    }).to_string());
                    let loading::PreparedWorld {
                        transport,
                        loaded,
                        build,
                        collision,
                        characters,
                        assets,
                        probes,
                        lease,
                        preparation_millis: _,
                        cache_hit: _,
                        lightmaps: _,
                    } = prepared;
                    // Recovery can restore the renderer's requested settings
                    // while the player's selection remains intact. A fresh
                    // level must stage its records with that current selection.
                    self.request_renderer_graphics();
                    self.renderer.install_prepared_precompiled(
                        &loaded,
                        build,
                        assets,
                        characters,
                        matches!(intent, LoadIntent::Graphics),
                        probes,
                    );
                    self.renderer.set_prepared_transport(transport);
                    self.pending_commit = Some(PendingCommit {
                        id,
                        loaded,
                        collision,
                        intent,
                        lease,
                    });
                    if let Err(error) = self.notify_upload() {
                        self.fatal_error = Some(error);
                        self.game.stop();
                        return;
                    }
                }
                Err(error) => self.recover_from_preparation_failure(id, &error),
            }
        }
        if self.pending_commit.is_some() {
            self.trace
                .record("upload_step_begin", self.load_generation, "");
            let committed = self.renderer.advance_prepared_install();
            self.trace
                .record("upload_step_end", self.load_generation, "");
            if committed {
                self.commit_loaded_world();
            }
        }
        // Idle cache retirement uses the worker but owns no loading UI.
        let phase = self.load_intent.and_then(|_| self.loader.phase());
        if phase != self.load_phase {
            self.load_phase = phase;
            if let Some(active_phase) = phase {
                let label = match active_phase {
                    loading::Phase::Queued => "Waiting for previous preparation",
                    loading::Phase::Reading => "Reading level and assets",
                    loading::Phase::Geometry => "Assembling compiled world",
                    loading::Phase::Lightmaps => "Decoding lightmaps",
                    loading::Phase::Collision => "Decoding collision",
                    loading::Phase::Characters => "Preparing characters",
                    loading::Phase::Visibility => "Preparing visibility",
                    loading::Phase::Ready => "Uploading level",
                };
                self.ui_state
                    .set_status(format!("{label}... Esc to cancel"), false);
                self.trace.record("phase", self.load_generation, label);
            }
        }
    }

    /// Handles one failed preparation.
    ///
    /// A failed *graphics* transition keeps the player's selection: the saved
    /// configuration is never rewritten to hide the failure (an unsupported
    /// quality is an actionable message, not a silent rollback), the renderer
    /// is left holding the resident configuration, and the failed request is
    /// remembered so the reconciliation does not retry it every frame. A
    /// failed *level* preparation still reveals the previous screen.
    fn recover_from_preparation_failure(&mut self, id: u64, error: &str) {
        if let (Some(actions), Some(source)) = (&self.actions, &self.load_source)
            && let Err(notify_error) = actions.notify(&format!("failed:{}", source.level_id()))
        {
            self.fatal_error = Some(notify_error);
            self.game.stop();
        }
        let graphics = matches!(self.load_intent, Some(LoadIntent::Graphics));
        if graphics {
            let spec = self.requested_graphics();
            self.failed_graphics = Some(spec);
            crate::engine::logging::warn(format!(
                "graphics change to {} did not apply: {error}",
                spec.quality.label()
            ));
        }
        self.renderer.abort_graphics_request();
        self.load_intent = None;
        self.load_source = None;
        self.trace_world("load_recovered");
        self.trace.record("failed", id, error);
        if graphics {
            // The error names the package and what it provides, which is the
            // actionable part: the selection stands and the player can pick a
            // supported one.
            self.ui_state.set_status(
                format!(
                    "Could not apply {}: {error}",
                    self.settings.graphics_spec().quality.label()
                ),
                true,
            );
        } else {
            self.ui_state
                .set_status(format!("Could not load level: {error}"), true);
        }
        crate::engine::logging::warn(format!("Could not load map: {error}"));
        if self.startup != StartupPhase::Ready {
            self.startup = StartupPhase::Ready;
        }
        self.game.set_app_state(self.load_return_state);
        self.clear_loading_inputs(graphics);
        self.game.reset_timing();
    }

    fn notify_upload(&mut self) -> Result<(), String> {
        self.ui_state
            .set_status("Uploading level... Esc to cancel".to_string(), false);
        if let (Some(actions), Some(pending)) = (&self.actions, &self.pending_commit) {
            actions.notify(&format!("upload:{}", pending.loaded.level.id))
        } else {
            Ok(())
        }
    }

    fn script_door_snapshot(&self) -> Vec<serde_json::Value> {
        self.game
            .doors()
            .iter()
            .map(|door| {
                serde_json::json!({
                    "id": door.def.id, "angle_degrees": door.angle(),
                    "phase": format!("{:?}", door.phase()), "obstructed": door.obstructed,
                    "locked": self.game.doors().is_locked(&door.def.id),
                })
            })
            .collect()
    }

    fn trace_player_body(&self) -> serde_json::Value {
        serde_json::json!({
            "feet_y": self.game.feet_y(), "body_height": self.game.body_height(),
            "eye_height": self.game.player_position.y - self.game.feet_y(),
            "grounded": self.game.grounded, "crouched": self.game.is_crouched(),
            "swimming": self.game.is_swimming(), "underwater": self.game.is_underwater(),
            "water_depth_level": self.game.water_depth_level(), "water_jump": self.game.water_exit.is_some(),
            "water_exit_support": self.game.water_exit.map(|exit| exit.support_y),
            "velocity_y": self.game.vertical_velocity, "velocity_xz": self.game.horizontal_velocity.to_array(),
            "fluid_velocity_xz": self.game.player_fluid_velocity().to_array(), "reset_count": self.game.reset_count()
        })
    }

    /// Explicit bounded pose/collision witnesses for native acceptance.
    fn trace_moving_solids(&self, fields: &mut serde_json::Map<String, serde_json::Value>) {
        drop(fields.insert(
            "animated_doors".into(),
            self.game.entities().animated_door_diagnostics(),
        ));
        drop(
            fields.insert(
                "physical_obstacles".into(),
                self.items
                    .world
                    .navigation_obstacles()
                    .map(|bounds| {
                        serde_json::json!([
                            [bounds.min_x, bounds.min_y, bounds.min_z],
                            [bounds.max_x, bounds.max_y, bounds.max_z]
                        ])
                    })
                    .collect::<Vec<_>>()
                    .into(),
            ),
        );
        drop(fields.insert("item_penetrations".into(), self.items.world.snapshots().iter().map(|body| {
        serde_json::json!({"key":body.key,"depth":self.items.world.body_penetration_depth(body.key)})
    }).collect::<Vec<_>>().into()));
    }

    fn trace_map_ownership(&self, fields: &mut serde_json::Map<String, serde_json::Value>) {
        if self.bench.enabled()
            && std::env::var("PLACES_BENCH_RESOURCE_TRACE").as_deref() == Ok("1")
        {
            drop(fields.insert("map_ownership".into(), serde_json::json!({
                    "active_prepared_bytes":self.displayed_lease.as_ref().map_or(0,loading::PreparedLease::retained_bytes),
                    "retired": self.retired_worlds.iter().map(|(id,probe)|serde_json::json!({"level":id,"ownership":probe.snapshot()})).collect::<Vec<_>>(),
                    "scripts":self.game.entities().script_resources(),
                    "ai_agents":self.game.entities().ai().len(),
                    "preparation_phase":self.loader.phase().map(|phase|format!("{phase:?}")),
                    "pending_commit":self.pending_commit.is_some(),
                })));
        }
    }

    fn trace_world(&mut self, event: &str) {
        if self.trace.will_record(event) {
            self.record_world_snapshot(event);
        }
    }

    fn record_world_snapshot(&mut self, event: &str) {
        let (renderer_level, renderer_quality, renderer_lightmaps) =
            self.renderer.installed_identity();
        let position = self.game.player_position.to_array();
        let doors = self.script_door_snapshot();
        let mut state = serde_json::json!({
            "physics": {"held": self.items.world.held(), "stats": self.items.world.stats(), "bodies": self.items.world.snapshots(), "contacts": self.items.world.contact_pairs(), "contact_impulses": self.items.world.contact_impulses(), "authored": self.items.authored, "lighting": self.item_lighting_snapshots()},
            "current_level_id": self.current_level.as_ref().map(|loaded| loaded.level.id.as_str()),
            "renderer_level_id": renderer_level,
            "walls": self.game.walls().len(), "interactables": self.game.interactables().len(),
            "routes": self.game.routes().len(), "triggers": self.game.volume_count(),
            "characters": self.renderer.character_count(), "dynamic_objects": self.renderer.dynamic_scene().len(),
            "entity_simulation": self.entity_simulation_snapshot(),
            "player_position": position, "quality": self.settings.quality_level().name(),
            "player_body": self.trace_player_body(),
            "view_direction": self.game.view_direction().to_array(),
            "interaction_target": self.interaction_target_snapshot(),
            "interaction_hand_visible": self.items.hand_visible(),
            "player_yaw": self.game.player_yaw, "player_pitch": self.game.player_pitch,
            "lighting": self.settings.lighting_quality().name(),
            "use_low_quality_lighting": self.settings.use_low_quality_lighting,
            "lightmaps": self.settings.lightmap_quality().name(), "reflections": self.settings.reflection_quality().name(),
            "renderer_quality": renderer_quality.name(), "renderer_lightmaps": renderer_lightmaps.name(),
            "renderer_lighting": self.renderer.graphics_applied().lighting.name(),
            "bloom": self.settings.bloom_enabled(), "window_focused": self.window_focused,
            "console_last": self.console.latest_message(), "console_scroll": self.console.scroll_offset(),
            "console_open": self.console.is_open(), "console_enabled": self.settings.developer_console, "noclip": self.game.noclip_enabled(),
            "relative_mouse": self.sdl.mouse().relative_mouse_mode(self.window),
            "input": format!("{:?}", self.input_handler.state()),
            "doors": doors, "light_states": self.game.light_states(),
            "state": format!("{:?}", self.game.app_state()), "frame": self.game.frame_count(),
            "filtering": self.settings.texture_filtering_preset(), "rss_bytes": perf::process_resident_bytes(),
            "gpu_timing": format!("{:?}", self.renderer.gpu_timing()),
        });
        if let Some(fields) = state.as_object_mut() {
            self.trace_map_ownership(fields);
            self.trace_moving_solids(fields);
            drop(fields.insert("crosshair_visible".into(), self.crosshair_visible().into()));
            drop(fields.insert(
                "crosshair_override".into(),
                serde_json::json!(self.console_runtime.crosshair_override),
            ));
            drop(fields.insert(
                "crosshair_map_default".into(),
                serde_json::json!(self.current_level.as_ref().map(|loaded| loaded.level.crosshair)),
            ));
            drop(fields.insert(
                "script_diagnostics".into(),
                serde_json::json!(self.game.entities().script_diagnostics()),
            ));
            drop(fields.insert(
                "carry_assemblies".into(),
                self.items.carry.diagnostics(self.renderer),
            ));
            drop(fields.insert("outcome".into(), serde_json::json!(self.outcome)));
            drop(fields.insert("guidance".into(), serde_json::json!(self.message)));
            drop(fields.insert("controls".into(), serde_json::json!({
                "flashlight_on": self.flashlight.is_on(),
                "settings_page": format!("{:?}", self.ui_state.settings_page), "settings_index": self.ui_state.settings_idx,
                "rebinding_action": self.ui_state.rebinding_action, "bindings": self.settings.bindings,
            })));
            drop(fields.insert("caustics".into(), self.renderer.caustics_diagnostics()));
            self.trace_native_window(fields);
            drop(fields.insert(
                "audio_voices".into(),
                serde_json::json!(self.audio.system.voice_snapshot()),
            ));
            drop(fields.insert("audio".into(), serde_json::json!(self.audio.system.stats())));
            drop(fields.insert("audio_settings".into(), serde_json::json!({"master":self.settings.master_volume,"effects":self.settings.effects_volume,"ambience":self.settings.ambient_volume,"muted":self.settings.muted})));
            drop(fields.insert("console_session".into(), self.console_runtime.snapshot()));
            let fixed_instances = self.current_level.as_ref().map(|loaded| {
                let ids=loaded.level.prop_instance_ids();
                loaded.level.props.iter().zip(ids).filter(|(prop,_)| !prop.is_physics_movable()).map(|(prop,id)| serde_json::json!({"id":id,"model":prop.model,"entity_live":self.game.entities().handle_of(&id).is_some()})).collect::<Vec<_>>()
            }).unwrap_or_default();
            drop(fields.insert("fixed_instances".into(), fixed_instances.into()));
            drop(fields.insert(
                "menu_beat".into(),
                serde_json::json!(self.ui_state.observed_menu_beat),
            ));
            drop(fields.insert("menu_ui".into(),serde_json::json!({"level_count":self.ui_state.level_entries.len(),"level_index":self.ui_state.level_select_idx,"scroll":self.ui_state.level_scroll,"description_page":self.ui_state.description_page,"status":self.ui_state.status_message,"status_is_error":self.ui_state.status_is_error,"status_page":self.ui_state.status_page,"credits":self.ui_state.credits.is_some()})));
            self.append_authoring_trace(fields);
            drop(fields.insert(
                "physics_clock".into(),
                serde_json::json!(self.items.world.simulation_state()),
            ));
            drop(fields.insert("representation".into(), serde_json::json!({"model":self.console_runtime.representation.selected_id(), "third_person":self.console_runtime.representation.is_active()})));
            drop(fields.insert(
                "trigger_labels".into(),
                serde_json::json!(
                        self.console_runtime
                            .visible_trigger_labels(self.game, self.renderer)
                            .iter()
                            .map(|label| label.id.as_str())
                            .collect::<Vec<_>>()
                    ),
            ));
            drop(fields.insert("flashlight_on".to_string(), self.flashlight.is_on().into()));

            self.append_character_trace(fields);
        }
        self.trace
            .record(event, self.load_generation, &state.to_string());
    }

    fn trace_native_window(&self, fields: &mut serde_json::Map<String, serde_json::Value>) {
        drop(fields.insert(
            "window_native_focus".into(),
            self.window.has_input_focus().into(),
        ));
        let flags = sdl3::video::WindowFlags::from(self.window.window_flags());
        let display = self
            .window
            .get_display()
            .ok()
            .and_then(|display| display.get_mode().ok());
        drop(fields.insert("window_native_state".into(), serde_json::json!({
            "id": self.window.id(), "flags": format!("{flags:?}"),
            "occluded": flags.contains(sdl3::video::WindowFlags::OCCLUDED),
            "logical": self.window.size(), "drawable": self.window.size_in_pixels(),
            "display": display.map(|mode| serde_json::json!({"size": [mode.w, mode.h], "hz": mode.refresh_rate, "hz_ratio": [mode.refresh_rate_numerator, mode.refresh_rate_denominator]})),
        })));
    }

    fn append_character_trace(&self, fields: &mut serde_json::Map<String, serde_json::Value>) {
        drop(fields.insert(
            "solver_resources".into(),
            self.items.world.solver_resources(),
        ));
        drop(fields.insert(
            "ragdolls".into(),
            serde_json::json!(self.items.world.ragdoll_snapshots()),
        ));
        let light_responses: Vec<_> = self.game.entities().light_response_observations().map(|observation| {
            serde_json::json!({
                "instance_id": observation.instance_id,
                "position": observation.position.to_array(),
                // api.actor reads this same component transform; retain the
                // renderer-bound frame separately to audit idle reset agreement.
                "script_position": observation.position.to_array(),
                "render_position": self.game.entity_frames().iter().find(|frame| frame.instance_id == observation.instance_id).and_then(|frame| frame.transform).map(|(rendered_position, _)| rendered_position.to_array()),
                "yaw": self.game.entity_frames().iter().find(|frame| frame.instance_id == observation.instance_id).and_then(|frame| frame.transform).map(|(_, yaw)| yaw),
                "intensity": observation.intensity,
                "active": observation.active,
                "opacity": self.game.entity_frames().iter().find(|frame| frame.instance_id == observation.instance_id).map(|frame| frame.opacity),
                "pose": self.game.entity_frames().iter().find(|frame| frame.instance_id == observation.instance_id).and_then(|frame| frame.cue.as_ref()).map(|cue| format!("{cue:?}")),
            })
        }).collect();
        drop(fields.insert("light_responses".to_string(), light_responses.into()));
        if self.trace.is_recording() {
            drop(
                fields.insert(
                    "character_animations".into(),
                    self.renderer
                        .character_animation_diagnostics(self.game.entity_frames()),
                ),
            );
        }
    }

    /// Snapshot existing entity state only for explicit developer trace events.
    fn entity_simulation_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "time": self.game.entities().sim_time(),
            "sequences": self.game.entities().active_sequences(),
            "agents": self.game.entities().ai().agents().iter().map(|agent| {
                serde_json::json!({
                    "id": agent.instance_id, "state": agent.state.name(),
                    "position": agent.position.to_array(), "yaw_degrees": agent.yaw_degrees,
                    "speed_mps": agent.speed_mps, "clock": agent.clock,
                    "path_age": agent.path_age, "waypoint": agent.current_waypoint,
                    "path_goal": agent.path_goal.map(|point| point.to_array()),
                    "path_waypoints": agent.path.waypoints.iter().map(Vec3::to_array).collect::<Vec<_>>(),
                    "path_complete": agent.path.complete, "stuck_seconds": agent.stuck_seconds,
                    "navigation_goal": agent.navigation_goal.map(|point| point.to_array()),
                    "path_queries": agent.path_queries,
                    "hidden_remaining": agent.hidden_remaining,
                    "opacity": agent.opacity(), "pose": format!("{:?}", agent.cue()),
                    "light_held": agent.light_look_target.is_some(),
                })
            }).collect::<Vec<_>>(),
        })
    }

    fn commit_loaded_world(&mut self) {
        let Some(PendingCommit {
            id,
            loaded,
            collision,
            intent,
            lease,
        }) = self.pending_commit.take()
        else {
            return;
        };
        if !matches!(intent, LoadIntent::Graphics) {
            self.console_runtime.reset_map_presentation();
            self.audio.clear_world();
            self.flashlight = flashlight::Power::Off;
            self.outcome = None;
            self.message = None;
            self.console_runtime
                .clear_given_props(self.renderer, &mut self.items.world);
        }
        self.close_console();
        let mut install_timer = perf::loading::StageTimer::new("game-install");
        self.trace.record("gpu_ready", id, &loaded.level.id);
        logging::info(format!("[loading] committed {}", loaded.level.id));
        self.failed_graphics = None;
        self.load_intent = None;
        if matches!(intent, LoadIntent::Graphics) {
            self.game.set_app_state(self.load_return_state);
        } else {
            spawn_level_demonstration(self.renderer, &loaded);
            install_timer.step("dynamic-content");
            *self.spawn_pos = game::spawn_position(&loaded.level);
            *self.spawn_yaw = loaded.level.spawn.yaw_degrees.to_radians();
            self.game
                .reset_level(*self.spawn_pos, *self.spawn_yaw, collision);
            install_timer.step("game-reset");
            self.install_physical_items(&loaded.level, &loaded.materials);
            install_timer.step("physical-items");
            self.update_item_triggers(true);
            if matches!(intent, LoadIntent::Play) {
                self.game.set_app_state(AppState::Playing);
            }
            self.ready_frames = 0;
            self.ready_seconds = 0.0;
        }
        // A commit or a graphics rebuild re-bakes the authored fixture states;
        // push the run's live switch states again so a rebuild can never
        // silently turn a switched-off light back on. An unchanged state is a
        // comparison and no GPU work.
        let light_states = self.game.light_states();
        if !light_states.is_empty() {
            self.renderer.apply_light_toggles(&light_states);
        }
        // A graphics-only rebuild replaces the resident resources in place: it
        // is the same visited world, so it emits no new scene-presented signal
        // and does not restart the ready-frame/capture counter. A real level
        // commit (menu background, play, or a replacement) does both.
        if !matches!(intent, LoadIntent::Graphics) {
            self.presentation.pending_scene = Some(self.load_generation);
        }
        // What the renderer actually installed is the renderer's own answer,
        // never an assumption that the current settings were applied: a commit
        // can deliver an older configuration (an install that started before a
        // newer selection). The state-driven reconciliation compares the
        // request with the renderer's real state and schedules what remains.
        log_prop_usage(self.renderer);
        let retiring_menu = self.menu_world() && matches!(intent, LoadIntent::Play);
        self.track_retired_world();
        *self.current_level = Some(loaded);
        self.displayed_lease = Some(lease);
        // Gameplay keeps its active lease; the tiny menu has no reason to
        // remain in the worker LRU once its simulation and GPU world retire.
        if retiring_menu && let Err(error) = self.loader.release_retained() {
            logging::warn(error);
        }
        self.ui_state.current_map = self
            .current_level
            .as_ref()
            .map(|world| world.level.name.clone());
        // The first committed world is the menu background; the normal menu
        // may be revealed from now on. It is also the fallback a later
        // replacement can be cancelled back to.
        self.startup = StartupPhase::Ready;
        self.load_source = None;
        self.load_phase = None;
        self.ui_state.clear_status();
        self.clear_loading_inputs(matches!(intent, LoadIntent::Graphics));
        self.game.reset_timing();
        if !matches!(intent, LoadIntent::Graphics) {
            self.initialize_menu_choreography();
        }
        if self.launch_overrides {
            self.launch_overrides = false;
            if let Some(spawn) = spawn_override_from_env() {
                apply_spawn_override(self.game, self.spawn_pos, self.spawn_yaw, spawn);
            }
            if matches!(intent, LoadIntent::Play) && pause_requested() {
                self.game.set_app_state(AppState::Paused);
            }
            apply_screen_request(self.game, self.ui_state);
        }
        install_timer.step("finalize");
        self.trace_world("world_committed");
        self.trace.record("ready", id, "");
        self.write_nav_debug(true);
    }

    /// Writes the developer navigation/AI inspection files when
    /// `PLACES_NAV_DEBUG` names a directory.
    ///
    /// The dump is generated from the live [`crate::engine::nav::NavMesh`] and
    /// [`crate::engine::ai::AiWorld`] the game is querying, not from a separate model,
    /// so it shows exactly the cells, regions, portals, paths, goals, contacts
    /// and catch radii the AI acts on. `force` writes the mesh once per world
    /// install; the AI state is refreshed periodically.
    fn write_nav_debug(&mut self, force: bool) {
        const AI_DUMP_INTERVAL_FRAMES: u64 = 30;
        let Some(dir) = self.nav_debug_dir.clone() else {
            return;
        };
        self.nav_debug_age = self.nav_debug_age.saturating_add(1);
        if !force && self.nav_debug_age < AI_DUMP_INTERVAL_FRAMES {
            return;
        }
        self.nav_debug_age = 0;
        if std::fs::create_dir_all(&dir).is_err() {
            self.nav_debug_dir = None;
            return;
        }
        if force && let Some(mesh) = self.game.navigation() {
            for class in 0..mesh.grid().classes.len() {
                let text = mesh.debug_ascii(class, self.game.doors());
                let path = dir.join(format!("navmesh-class{class}.txt"));
                if std::fs::write(&path, text).is_err() {
                    self.nav_debug_dir = None;
                    return;
                }
            }
        }
        let aimed = self
            .game
            .interaction_target()
            .and_then(|index| self.game.interactables().get(index))
            .map_or_else(|| "-".to_string(), |item| item.id.clone());
        let mut report = format!(
            "frame={} sim_time={:.2} stimuli={} sequences={} aimed={}\n",
            self.game.frame_count(),
            self.game.entities().sim_time(),
            self.game.entities().stimuli().len(),
            self.game.entities().active_sequences(),
            aimed,
        );
        report.push_str(&self.game.entities().ai().debug_report());
        if std::fs::write(dir.join("ai-state.txt"), report).is_err() {
            self.nav_debug_dir = None;
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
                        .set_status("No new maps found in the import folder.", false);
                }
            }
            Err(err) => {
                self.refresh_level_entries();
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
                // Every newly opened settings screen starts with the Advanced
                // group collapsed; the state is never persisted.
                self.ui_state.advanced_expanded = false;
                self.goto(AppState::PauseSettings);
            }
            2 => self.goto(AppState::MainMenu),
            _ => {}
        }
    }

    fn sync_menu_images(&mut self) {
        if self.game.app_state() == AppState::LevelSelect
            && self.ui_state.description_index != Some(self.ui_state.level_select_idx)
        {
            self.ui_state.description_index = Some(self.ui_state.level_select_idx);
            let description = self
                .level_manager
                .menu_description(self.ui_state.level_select_idx);
            if description != self.ui_state.level_description {
                self.ui_state.level_description = description;
                self.ui_state.description_page = 0;
            }
        }
        let images = if self.game.app_state() == AppState::LevelSelect
            && !self.ui_state.loading_map
            && !self.ui_state.status_is_error
            && self.ui_state.credits.is_none()
            && self.startup == StartupPhase::Ready
        {
            ui::menu_layout(
                self.game.app_state(),
                self.ui_state,
                self.settings,
                self.display_status,
            )
            .rows
            .iter()
            .filter_map(|row| {
                Some(render::UiImage {
                    bounds: ui::level_icon_bounds(row),
                    image: self.level_manager.menu_thumbnail(row.index)?,
                })
            })
            .collect()
        } else {
            Vec::new()
        };
        self.renderer.set_ui_images(images);
    }

    /// Submits the UI pass: cached menu/settings geometry, the debug overlay
    /// when visible, and — while playing — the world-anchored interaction
    /// labels and the aimed-at prompt through the same text pipeline.
    fn render_guidance(&mut self) -> bool {
        let Some((title, lines)) = &self.message else {
            return false;
        };

        self.ui_scratch.clear();
        let wrapped = ui::wrap_guidance(lines);
        let pages = wrapped.len().div_ceil(6).max(1);
        let heading = if pages > 1 {
            format!("{title} ({}/{pages})", self.message_page.saturating_add(1))
        } else {
            title.clone()
        };
        let start = self.message_page.saturating_mul(6).min(wrapped.len());
        ui::append_guidance(
            self.ui_scratch,
            &heading,
            wrapped
                .get(start..start.saturating_add(6).min(wrapped.len()))
                .unwrap_or_default(),
        );
        if pages > 1 {
            ui::draw_text(
                self.ui_scratch,
                "PAGE UP / PAGE DOWN - READ MORE",
                24.0,
                205.0,
                0.875,
                [0.6, 0.68, 0.7],
            );
        }
        self.renderer.render_ui(self.ui_scratch);
        true
    }

    fn append_play_labels(
        &mut self,
        drawable: DrawableSize,
        cam_pos: Vec3,
        cam_yaw: f32,
        cam_pitch: f32,
    ) {
        if self.crosshair_visible() {
            ui::append_crosshair(self.ui_scratch, drawable);
        }
        let item_hint = self.items.hand_visible();
        world_labels::append_world_labels_with_hint(
            self.ui_scratch,
            self.game,
            &render::RenderCamera::new(cam_pos, cam_yaw, cam_pitch, self.settings.fov_degrees),
            drawable,
            false,
            &self.settings.bindings.interact,
        );
        if item_hint {
            ui::append_interaction_hint(self.ui_scratch, &self.settings.bindings.interact);
        }
        self.items.append_release_hint(self.ui_scratch);
        if self.console_runtime.trigger_labels_visible() {
            self.items.append_authoring_debug(
                self.ui_scratch,
                self.game,
                &render::RenderCamera::new(cam_pos, cam_yaw, cam_pitch, self.settings.fov_degrees),
                drawable,
            );

            world_labels::append_trigger_labels(
                self.ui_scratch,
                &self
                    .console_runtime
                    .visible_trigger_labels(self.game, self.renderer),
                &render::RenderCamera::new(cam_pos, cam_yaw, cam_pitch, self.settings.fov_degrees),
                drawable,
            );
        }
    }

    fn submit_ui(
        &mut self,
        drawable: DrawableSize,
        cam_pos: Vec3,
        cam_yaw: f32,
        cam_pitch: f32,
        skip_render: bool,
    ) {
        self.sync_menu_images();
        if skip_render {
            // `PLACES_BENCH_NORENDER=1`: measure the presentation path alone.
            return;
        }
        if self.startup != StartupPhase::Ready {
            // The ordinary menu is not drawn until the requested level backs
            // it. Until then the preparation screen is the visible state; it
            // carries the live status and the window stays responsive.
            let vertices = ui::loading_geometry(self.ui_state, APP_VERSION);
            self.renderer.render_ui(&vertices);
            return;
        }
        if self.render_guidance() {
            return;
        }
        if let Some((won, title, detail)) = &self.outcome {
            self.ui_scratch.clear();
            ui::append_run_feedback(self.ui_scratch, *won, title, detail);
            self.renderer.render_ui(self.ui_scratch);
            return;
        }
        let ui_vertices = self.ui_cache.get(
            self.game.app_state(),
            self.ui_state,
            self.settings,
            self.display_status,
            APP_VERSION,
        );
        // Labels are only appended while actually playing, never in a menu or a
        // pause; the overlay is appended on demand.
        let show_labels = self.game.app_state() == AppState::Playing && !self.console.is_open();
        if show_labels || self.perf_overlay.is_visible() || self.console.is_open() {
            self.ui_scratch.clear();
            self.ui_scratch.extend_from_slice(ui_vertices);
            if show_labels {
                self.append_play_labels(drawable, cam_pos, cam_yaw, cam_pitch);
            }
            if self.perf_overlay.is_visible() {
                self.ui_scratch
                    .extend_from_slice(self.perf_overlay.cached_vertices());
            }
            if self.game.noclip_enabled() && !self.console.is_open() {
                ui::draw_text(
                    self.ui_scratch,
                    "NOCLIP",
                    12.0,
                    12.0,
                    1.0,
                    [1.0, 0.88, 0.62],
                );
            }
            if self.console.is_open() {
                self.ui_scratch
                    .extend_from_slice(self.console.vertices(self.game.noclip_enabled()));
            }
            self.renderer.render_ui(self.ui_scratch);
        } else {
            self.renderer.render_ui(ui_vertices);
        }
    }

    fn trace_lighting_capture_state(&self) {
        if !self.bench.enabled() {
            return;
        }
        let player = self.game.player_position;
        let eye = self
            .bench
            .camera_position_override()
            .unwrap_or_else(|| player.to_array());
        let (yaw, pitch) = self.bench.camera_override().unwrap_or_else(|| {
            (
                self.game.player_yaw.to_degrees(),
                self.game.player_pitch.to_degrees(),
            )
        });
        let doors: Vec<_> = self
            .game
            .doors()
            .iter()
            .map(|door| {
                serde_json::json!({
                    "id": door.def.id, "angle_degrees": door.angle(),
                    "phase": format!("{:?}", door.phase()), "obstructed": door.obstructed,
                })
            })
            .collect();
        crate::engine::logging::info(format!(
            "[lighting-capture-state] {}",
            serde_json::json!({
                "ready_frame": self.ready_frames,
                "simulation_seconds": self.ready_seconds,
                "fixed_delta_seconds": self.bench.fixed_delta_seconds(),
                "player_eye": player.to_array(),
                "render_eye": eye,
                "render_yaw_pitch_degrees": [yaw, pitch],
                "fov_degrees": self.frame_camera().vertical_fov_degrees,
                "doors": doors,
                "light_states": self.game.light_states(),
            })
        ));
    }

    /// Frame-based screenshots also support the settled source-only menu.
    /// Timed lighting captures continue to require a prepared world.
    fn capture_ready_screen(&mut self, world_ready: bool, frame_begin: Instant) {
        let capture_ready = world_ready
            || ((self.capture_path.is_some() || !self.items.captures.is_empty())
                && self.capture_at_seconds.is_none()
                && self.current_level.is_none()
                && self.load_intent.is_none()
                && self.startup == StartupPhase::Ready
                && matches!(
                    self.game.app_state(),
                    AppState::MainMenu | AppState::LevelSelect | AppState::Settings
                ));
        if capture_ready {
            self.ready_frames = self.ready_frames.saturating_add(1);
            self.capture_item_frame();
            self.record_video_frame(frame_begin);
        }
        self.capture_if_due(capture_ready);
    }

    /// Takes the one-shot `PLACES_CAPTURE` frame when it is due.
    ///
    /// The capture waits for a ready world or a settled source-only main menu
    /// so it records the finished screen, never a preparation frame. It is due
    /// at the ready frame named by
    /// `PLACES_CAPTURE_FRAME` or the ready-world second named by
    /// `PLACES_CAPTURE_TIME` (the first frame at or after it).
    fn capture_if_due(&mut self, ready: bool) {
        if ready && let Some(path) = self.bench.lighting_capture_at(self.ready_frames) {
            if path.exists() {
                self.fatal_error =
                    Some("Native lighting sequence refuses an existing capture".to_string());
                self.game.stop();
                return;
            }
            self.trace_lighting_capture_state();
            write_capture(self.renderer, &path);
            crate::engine::logging::info(format!(
                "[lighting-sequence] capture_attempt ready_frame={} path={}",
                self.ready_frames,
                path.display()
            ));
        }
        if !ready
            || !self.items.captures.is_empty()
            || !self.actions.as_ref().is_none_or(actions::Actions::complete)
        {
            return;
        }
        let due = match self.capture_at_seconds {
            Some(seconds) => self.ready_seconds >= seconds,
            None => self.ready_frames >= self.capture_at_frame,
        };
        if !due {
            return;
        }
        let Some(path) = self.capture_path.take() else {
            return;
        };
        self.trace_lighting_capture_state();
        write_capture(self.renderer, &path);
        perf::startup_mark("capture readback");
        self.game.stop();
    }

    fn notify_presented(&mut self, ready: bool, presented: bool) {
        if ready
            && presented
            && let Some(generation) = self.presentation.pending_scene.take()
            && let Some(loaded) = self.current_level.as_ref()
        {
            self.trace
                .record("scene_presented", generation, &loaded.level.id);
        }
        if ready
            && self.ready_frames == 1
            && let Some(loaded) = self.current_level.as_ref()
            && let Some(actions) = &self.actions
            && let Err(error) = actions.notify(&format!("ready:{}", loaded.level.id))
        {
            self.fatal_error = Some(error);
            self.game.stop();
        }
        if let Some(actions) = &mut self.actions
            && let Err(error) = actions.join_finished()
        {
            self.fatal_error = Some(error);
            self.game.stop();
        }
    }

    /// Presents or yields when the native surface is temporarily unavailable.
    fn present_frame(&mut self, frame_begin: Instant) -> bool {
        let presented = !self.bench.skip_swap() && self.renderer.present(self.window);
        if !self.bench.skip_swap() && !presented {
            // An occluded surface can reject acquisition immediately despite
            // a nonzero drawable and VSync. Yield instead of spinning until
            // the window becomes available; successful frames keep VSync pacing.
            let pause = std::time::Duration::from_millis(16).saturating_sub(frame_begin.elapsed());
            if !pause.is_zero() {
                std::thread::sleep(pause);
            }
        }
        presented
    }

    /// Draws the frame, handles the one-shot capture, presents and records it.
    fn frame_camera(&self) -> render::RenderCamera {
        let representation_camera = self.console_runtime.representation.camera(
            Vec3::new(
                self.game.player_position.x,
                self.game.feet_y(),
                self.game.player_position.z,
            ),
            self.game.player_yaw,
            self.game.player_pitch,
            self.settings.fov_degrees,
            |start, end, radius| {
                let static_end = self.game.camera_sweep(start, end, radius);
                self.items
                    .world
                    .camera_sweep(start, static_end, radius)
                    .unwrap_or(static_end)
            },
        );
        let (cam_pos, cam_yaw, cam_pitch) = match self.game.app_state() {
            AppState::Playing | AppState::Paused | AppState::PauseSettings => representation_camera
                .map_or(
                    (
                        self.game.player_position,
                        self.game.player_yaw,
                        self.game.player_pitch,
                    ),
                    |camera| (camera.position, camera.yaw, camera.pitch),
                ),
            AppState::MainMenu | AppState::LevelSelect | AppState::Settings => {
                // Static menu background camera
                (
                    *self.spawn_pos,
                    *self.spawn_yaw,
                    if self.menu_world() {
                        -5.0_f32.to_radians()
                    } else {
                        0.0
                    },
                )
            }
        };
        // `PLACES_CAMERA=yaw[,pitch]` pins the camera so a hardware benchmark
        // measures the same view twice; it never changes gameplay.
        let (render_yaw, render_pitch) = match self.bench.camera_override() {
            Some((yaw, pitch)) => (yaw.to_radians(), pitch.to_radians()),
            None => (cam_yaw, cam_pitch),
        };
        let render_position = self
            .bench
            .camera_position_override()
            .map_or(cam_pos, Vec3::from_array);

        render::RenderCamera::new(
            render_position,
            render_yaw,
            render_pitch,
            if self.menu_world() {
                55.0
            } else {
                self.settings.fov_degrees
            },
        )
    }

    fn render_and_present(&mut self, frame_begin: Instant, frame_update_done: Instant) {
        // Use the physical drawable size (SDL3's window size in pixels), not
        // the logical window size, so HiDPI (Retina) backing scale and monitor
        // changes are handled automatically.
        let (drawable_width, drawable_height) = self.window.size_in_pixels();
        let drawable = DrawableSize::new(drawable_width, drawable_height);

        // Minimized/hidden windows report a zero-sized drawable. Skip rendering
        // rather than submitting a zero-sized frame, and keep timing fresh so
        // restoring does not jump.
        if drawable.is_empty() {
            self.game.reset_timing();
            std::thread::sleep(std::time::Duration::from_millis(16));
            return;
        }

        let _drawable_size_changed = self.renderer.set_drawable_size(drawable);

        // Render scene
        let frame_camera = self.frame_camera();
        let render_position = frame_camera.position;
        let render_yaw = frame_camera.yaw;
        let render_pitch = frame_camera.pitch;

        let skip_render = self.bench.skip_render();
        if !skip_render {
            let camera = self.prepare_flashlight(render_position, render_yaw, render_pitch);
            self.renderer.render_scene(camera);
        }
        let frame_render_done = Instant::now();
        if self.game.frame_count() == 1 && !skip_render {
            perf::startup_mark("first scene attempted");
        }
        // Apply a changed texture filtering setting to existing GL textures
        // without re-uploading their pixel data.
        if self.settings.texture_filtering != *self.applied_filtering {
            self.renderer
                .set_texture_filtering(&self.settings.texture_filtering);
            self.applied_filtering
                .clone_from(&self.settings.texture_filtering);
        }

        // Menu/settings UI geometry is cached and only rebuilt when its inputs
        // change. Floating interaction labels and the aimed-at prompt are
        // appended to the same submission.
        self.submit_ui(
            drawable,
            render_position,
            render_yaw,
            render_pitch,
            skip_render,
        );
        // `PLACES_BENCH_FINISH=1`: force submitted GPU work to drain before the
        // swap timing point, so `render_ms` is renderer completion time rather
        // than "how much of the frame the driver happened to absorb".
        if self.bench.finish_before_swap() {
            self.renderer.finish();
        }
        let frame_ui_done = Instant::now();
        if self.game.frame_count() == 1 {
            perf::startup_mark("first UI attempted");
        }

        let ready = self.current_level.is_some() && self.load_intent.is_none();
        self.capture_ready_screen(ready, frame_begin);

        // Swap window buffer (double buffered, VSync synchronized). The
        // renderer presents its acquired surface texture.
        let presented = self.present_frame(frame_begin);
        let frame_swap_done = Instant::now();
        self.notify_presented(ready, presented);
        self.trace.record(
            if presented {
                "present"
            } else {
                "surface_not_presented"
            },
            self.load_generation,
            if ready { "ready" } else { "loading" },
        );
        if presented && !self.presentation.has_presented {
            self.presentation.has_presented = true;
            perf::startup_mark("first presented");
            // A summarized phase table when PLACES_VERBOSE is on; a release
            // launch with no developer switches prints nothing.
            perf::startup_report();
        }

        if self.bench.enabled() && ready {
            self.bench.set_gpu_timing(self.renderer.gpu_timing());
            self.bench.record_frame(
                frame_begin,
                frame_update_done,
                frame_render_done,
                frame_ui_done,
                frame_swap_done,
                self.renderer.render_stats(),
            );
            if self.bench.is_complete()
                && self.actions.as_ref().is_none_or(actions::Actions::complete)
                && self.items.captures.is_empty()
            {
                self.trace_world("benchmark_complete");
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
    logging::event(format!(
        "[settings] quality {} (saved {}){} | lighting {} | bloom {} | reflections {}{} | lightmaps {}{} | vsync {} | filtering {} | window {} {}",
        settings.quality_level().name(),
        settings.quality,
        if settings.quality_overridden() {
            " [startup override]"
        } else {
            ""
        },
        settings.lighting_quality().name(),
        on_off(settings.bloom_enabled()),
        settings.reflection_quality().name(),
        if settings.reflection_quality_overridden() {
            " [startup override]"
        } else {
            ""
        },
        settings.lightmap_quality().name(),
        if settings.lightmap_quality_overridden() {
            " [startup override]"
        } else {
            ""
        },
        on_off(settings.vsync_enabled()),
        settings.texture_filtering_preset(),
        settings.window_mode().label(),
        display::format_resolution(settings.window_size()),
    ));
}

const fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// Boots SDL, logs resolved paths, loads the
/// persisted settings (with any startup overrides) and creates the window.
fn bootstrap() -> Result<(Sdl, VideoSubsystem, Window, Settings, Bench), String> {
    log_package()?;
    perf::startup_mark("package root");
    // The app identifier is what X11/Wayland use to associate the window with
    // Places; it must be set before `sdl3::init()`.
    sdl3::set_app_metadata(
        Some("Places"),
        Some(APP_VERSION),
        Some("io.github.csd113.places"),
    )
    .map_err(|error| format!("Failed to set app metadata: {error}"))?;

    // The benchmark harness is parsed first because `PLACES_VSYNC` is its
    // switch and has to be folded into the settings before the swap interval is
    // configured.
    let bench = Bench::new();
    let mut settings = Settings::load_or_default()?;
    settings.apply_startup_overrides(bench.vsync_override());
    settings.ensure_saved()?;
    log_effective_settings(&settings);
    perf::startup_mark("settings");

    let (sdl_context, video_subsystem, window) = create_sdl_and_window(&mut settings)?;
    perf::startup_mark("window");
    // Boot applies every setting explicitly (window, swap interval, renderer),
    // so no pending work is owed after it.
    let _cleared_pending_apply = settings.take_pending_apply();
    Ok((sdl_context, video_subsystem, window, settings, bench))
}

/// Selection depends only on installed entries, never a particular map id.
fn select_initial_level(
    entries: &[loader::LevelEntry],
    requested: Option<&str>,
) -> (Option<loader::LevelEntry>, bool) {
    let normalized = requested.map(str::trim).filter(|value| !value.is_empty());
    normalized.map_or_else(
        || (None, false),
        |value| {
            let entry = entries
                .iter()
                .find(|entry| entry.id == value)
                .or_else(|| {
                    entries
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(value))
                })
                .cloned();
            let direct = entry.is_some();
            (entry, direct)
        },
    )
}

fn initial_level(level_manager: &loader::LevelManager) -> (Option<loader::LevelEntry>, bool) {
    let requested = std::env::var("PLACES_LEVEL").ok();
    let entries = if requested
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        level_manager.entries()
    } else {
        level_manager.menu_entries()
    };
    let selection = select_initial_level(entries, requested.as_deref());
    if requested
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && !selection.1
    {
        logging::warn(
            "Requested map is not installed. Open Explore to choose or import a .placesmap.",
        );
    }
    if selection.0.is_none() {
        (menu_entry().ok(), false)
    } else {
        selection
    }
}

/// The small non-playable scene is an application resource, outside gameplay discovery.
fn menu_entry() -> Result<loader::LevelEntry, String> {
    Ok(loader::LevelEntry {
        id: "places_menu".to_string(),
        name: "A few places, all at once".to_string(),
        author: "Places".to_string(),
        source_type: loader::LevelSourceType::Bundled,
        path: crate::engine::paths::assets()?.join("shared/menu/scene.bin"),
    })
}

/// Opens only the resolved import directory, without a shell or arbitrary command.
fn open_import_folder() -> Result<(), String> {
    let directory = crate::engine::paths::user()?.imports();
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer.exe"
    } else {
        "xdg-open"
    };
    let _child = std::process::Command::new(program)
        .arg(&directory)
        .spawn()
        .map_err(|error| format!("Cannot open {}: {error}", directory.display()))?;
    Ok(())
}

#[expect(clippy::print_stdout, reason = "explicit path/import CLI output")]
fn dispatch_paths_cli(args: &[String]) -> Result<bool, String> {
    if args.first().is_some_and(|arg| arg == "--paths") {
        if args.len() != 1 {
            return Err("Usage: places --paths".into());
        }
        let user = crate::engine::paths::user()?;
        println!(
            "{}",
            serde_json::json!({
                "assets": crate::engine::paths::assets()?, "data": user.data,
                "config": user.config, "cache": user.cache, "settings": user.settings(),
                "levels": user.levels(), "import": user.imports(), "projects": user.projects(),
                "saves": user.saves()
            })
        );
        return Ok(true);
    }
    if args.first().is_some_and(|arg| arg == "--import") {
        let [_, source] = args else {
            return Err("Usage: places --import <file.placesmap>".into());
        };
        let mut manager = loader::LevelManager::new()?;
        let entry = manager.import_file(Path::new(source))?;
        println!("{}\t{}", entry.id, entry.path.display());
        return Ok(true);
    }
    Ok(false)
}

/// Headless player CLI: `places --list-levels` prints what discovery found and
/// exits before any window is created.
///
/// One tab-separated row per playable entry: level id, source (`bundled`,
/// `installed`) and package path.
/// Includes internal project members hidden by `menu_entry_only`, so diagnostic
/// selection and complete installed membership can be checked without a display
/// or GPU. Returns true when the flag was handled.
// Startup CLI output has no logger to route through.
#[expect(
    clippy::print_stdout,
    reason = "Startup CLI output has no logger to route through."
)]
fn dispatch_list_levels_cli(args: &[String]) -> Result<bool, String> {
    if !args.iter().any(|argument| argument == "--list-levels") {
        return Ok(false);
    }
    let manager = loader::LevelManager::new()?;
    for entry in manager.entries() {
        println!(
            "{}\t{}\t{}",
            entry.id,
            entry.source_type.label(),
            entry.path.display()
        );
    }
    Ok(true)
}

/// Runs the Places player: window, renderer, discovery, loading and the frame loop.
///
/// Native arguments select installed-level discovery before opening a window.
/// Legacy source geometry commands are dispatched by the executable entry point.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let preparation = crate::engine::assets::validate_installation()
        .and_then(|()| crate::engine::paths::initialize_user().map(|_| ()));
    if let Err(message) = preparation {
        if args.is_empty() {
            diagnostics::show_notice("Places could not start", &message);
        }
        return Err(message.into());
    }
    if dispatch_paths_cli(args)? || dispatch_list_levels_cli(args)? {
        return Ok(());
    }
    let interrupted = diagnostics::start();
    let result = run_player(interrupted);
    let error = result.as_ref().err().map(ToString::to_string);
    if let Some(message) = error.as_deref() {
        diagnostics::show_notice("Places could not continue", message);
    }
    diagnostics::finish(error.as_deref());
    result
}

fn native_inputs(
    sdl: &Sdl,
    window: &Window,
) -> Result<(EventPump, Option<actions::Actions>), String> {
    let pump = sdl
        .event_pump()
        .map_err(|error| format!("Failed to init event pump: {error}"))?;
    let action_events = sdl.event().map_err(|error| error.to_string())?;
    actions::Actions::from_env(&action_events, window.id()).map(|actions| (pump, actions))
}

fn begin_player_trace() -> perf::loading::LoadTrace {
    let mut trace = perf::loading::LoadTrace::new();
    trace.record("entry", 0, "");
    perf::startup_begin();
    trace
}

fn run_player(interrupted: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut trace = begin_player_trace();
    let (sdl_context, video_subsystem, mut window, mut settings, mut bench) = bootstrap()?;
    diagnostics::previous_session_notice(interrupted);

    let mut level_manager = loader::LevelManager::new()?;
    log_asset_catalog(&level_manager);
    perf::startup_mark("asset catalog");
    let mut renderer = create_renderer(&window, &settings, &bench)?;
    configure_vsync(&mut renderer, &mut bench, &settings);
    trace.record("window_ready", 0, "");

    let (mut event_pump, actions) = native_inputs(&sdl_context, &window)?;
    let mut input_handler = InputHandler::new();
    let mut spawn_pos = Vec3::ZERO;
    let mut spawn_yaw = 0.0;
    let mut game = Game::new(spawn_pos, spawn_yaw, CollisionWorld::default());
    let mut current_level = None;
    let mut display_status = DisplayStatus::default();
    let (initial_entry, direct) = initial_level(&level_manager);
    let loader = loading::Loader::new(loader::LevelManager::new()?)?;
    // `PLACES_STATE_LOG=file.csv` records `frame,x,y,z,yaw,pitch` while the
    // game runs, so control and movement checks can assert real input results
    // from a running build instead of inferring them from screenshots.
    let mut state_log = open_state_log();

    let mut ui_state = new_ui_state(&level_manager);
    apply_screen_request(&mut game, &mut ui_state);
    let mut perf_overlay = PerfOverlay::new();
    let mut ui_cache = UiGeometryCache::new();
    let mut ui_scratch: Vec<Vertex> = Vec::new();
    let mut applied_filtering = settings.texture_filtering.clone();
    let mut capture_path = capture_path_from_env();
    let capture_at_frame = capture_frame_from_env();
    let capture_at_seconds = capture_time_from_env();

    let initial_lightmaps = settings.lightmap_quality();
    let mut frame_loop = FrameLoop {
        diagnostic_tick: Instant::now(),
        diagnostic_state: None,
        window: &mut window,
        sdl: &sdl_context,
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
        console: console::Console::default(),
        console_runtime: console::runtime::Runtime::default(),
        flashlight: flashlight::Power::Off,
        outcome: None,
        message: None,
        message_page: 0,
        items: item_runtime::Items::new(),
        audio: audio_runtime::Audio::new(&sdl_context),
        video: video::Recording::from_env(),
        ui_cache: &mut ui_cache,
        ui_scratch: &mut ui_scratch,
        applied_filtering: &mut applied_filtering,
        capture_path: &mut capture_path,
        capture_at_frame,
        capture_at_seconds,
        state_log: &mut state_log,
        spawn_pos: &mut spawn_pos,
        spawn_yaw: &mut spawn_yaw,
        current_level: &mut current_level,
        display_status: &mut display_status,
        window_apply_in_flight: false,
        last_logged_window: (0, 0, 0, 0),
        window_focused: true,
        fatal_error: None,
        loader,
        load_intent: None,
        pending_commit: None,
        displayed_lease: None,
        retired_worlds: std::collections::VecDeque::new(),
        load_source: None,
        load_generation: 0,
        load_return_state: AppState::MainMenu,
        load_phase: None,
        load_lightmaps: initial_lightmaps,
        startup: StartupPhase::Preparing,
        launch_overrides: true,
        ready_frames: 0,
        presentation: PresentationState::default(),
        trace,
        actions,
        failed_graphics: None,
        graphics_hint: None,
        nav_debug_dir: std::env::var_os("PLACES_NAV_DEBUG").map(PathBuf::from),
        nav_debug_age: 0,
        dev_interactions: interact_overrides_from_env(),
        move_script: move_script_from_env(),
        ready_seconds: 0.0,
    };
    frame_loop.start(initial_entry, direct);

    if let Some(error) = frame_loop.fatal_error.take() {
        return Err(error.into());
    }

    if bench.enabled() {
        bench.finish();
    }

    Ok(())
}

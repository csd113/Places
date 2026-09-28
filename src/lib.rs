pub mod ai;
#[cfg(test)]
mod architecture_audit;
pub mod assets;
pub mod bench;
pub mod canonical_json;
pub mod collision;
pub mod collision_index;
pub mod compiler;
pub mod display;
pub mod door;
pub mod entities;
pub mod entity;
pub mod font;
pub mod game;
pub mod geometry_check;
pub mod gltf;
pub mod input;
pub mod interact;
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
mod lighting_partition_audit;
#[cfg(test)]
mod lighting_vertical_audit;
pub mod loader;
mod loading;
pub mod logging;
pub mod materials;
pub mod nav;
pub mod package;
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
#[cfg(test)]
mod zoo_audit;

use std::cmp::Ordering;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glam::Vec3;
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use sdl3::video::{Display, FullscreenType, Window};
use sdl3::{EventPump, Sdl, VideoSubsystem};

use bench::Bench;
use display::DisplayStatus;
use game::{AppState, CollisionWorld, Game};
use input::{InputHandler, MenuNavEvent, keycode_to_str};
use perf::PerfOverlay;
use render::{DrawableSize, GraphicsTransition, Renderer, Vertex};
use settings::{Settings, WindowMode};
use ui::{
    SettingsAction, SettingsPage, UiGeometryCache, UiState, activate_settings_item,
    activate_settings_row,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often `PLACES_STATE_LOG` records the player state, in frames.
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

/// Prints what the level's props and baked lighting cost: decoded models,
/// texture memory, draw calls, level-build time and the baked room baselines.
///
/// Developer telemetry: only printed when `PLACES_VERBOSE` is set.
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
#[allow(clippy::print_stdout)]
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

/// Root of the running installation.
///
/// The package root is the directory that owns `assets/`. It is found through
/// [`crate::assets::resolved_package_roots`], which searches `$PLACES_ASSET_ROOT`,
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
/// Developer telemetry: only printed when `PLACES_VERBOSE` is set.
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
/// The window opens at the saved windowed resolution ([`crate::settings::DEFAULT_WINDOW_WIDTH`]
/// x [`crate::settings::DEFAULT_WINDOW_HEIGHT`] on a fresh install), reduced to
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
    builder.position_centered().resizable().high_pixel_density();
    if settings.window_mode() == WindowMode::Fullscreen {
        // A window asked for fullscreen without an explicit display mode is
        // borderless desktop fullscreen.
        builder.fullscreen();
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
    // Floating props are level content, not a demonstration: every level that
    // authors `float` gets them, and the spawn is idempotent. It runs after
    // the demo spawn because that one clears the whole dynamic scene first.
    renderer.set_floating_props(&loaded.level);
    // Ambient effects are level content too: the install already built the
    // steam plumes from the resolved material table, and this idempotent call
    // is the engine's explicit hand-off.
    renderer.set_level_effects(&loaded.level);
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
    renderer.set_lightmap_quality(settings.lightmap_quality());
    renderer.set_reflection_quality(settings.reflection_quality());
    // Bloom and Texture Filtering are independent player preferences too (with
    // their `PLACES_*` startup overrides already folded in). Filtering is
    // recorded before the level upload so the load-time texture diagnostic
    // names the preset actually in force.
    renderer.set_bloom_enabled(settings.bloom_enabled());
    renderer.set_texture_filtering(settings.texture_filtering_preset());
    renderer.set_culling(!bench.no_cull());
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
        .unwrap_or(1)
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

/// The parsed `PLACES_SPAWN` override, if set.
fn spawn_override_from_env() -> Option<[f32; 4]> {
    std::env::var("PLACES_SPAWN")
        .ok()
        .and_then(|value| parse_spawn_override(&value))
}

/// Opens the `PLACES_STATE_LOG` CSV file, reporting why it cannot be used.
// Startup CLI output that has no logger to route through.
#[allow(clippy::print_stderr)]
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
#[allow(clippy::print_stdout, clippy::print_stderr)]
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
#[derive(Clone, Copy)]
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
}

/// Where startup is in bringing up the first world.
///
/// The first preparation is essential: it is the only thing that can give the
/// menu a background, so it is neither cancellable nor skippable, while a
/// later replacement is cancellable back to the committed world.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StartupPhase {
    /// First world requested and not yet committed: menu hidden, input gated.
    Preparing,
    /// The first preparation failed and the default level is being retried.
    Retrying,
    /// A world has been committed; the ordinary menu experience applies.
    Ready,
}

#[derive(Default)]
struct PresentationState {
    has_presented: bool,
    pending_scene: Option<u64>,
}

struct FrameLoop<'a> {
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
    actions: Option<perf::actions::Actions>,
    applied_graphics_settings: Settings,
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
}

impl FrameLoop<'_> {
    /// Requests the first world and runs the frame loop to completion.
    fn start(&mut self, initial_entry: Option<loader::LevelEntry>, direct: bool) {
        self.request_load(
            initial_entry.map_or(loading::Source::Default, loading::Source::entry),
            if direct {
                LoadIntent::Play
            } else {
                LoadIntent::Background
            },
        );
        self.run();
    }

    /// Runs frames until the game stops.
    fn run(&mut self) {
        while self.game.is_running() {
            self.frame();
        }
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
            self.renderer
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
            self.renderer.present(self.window);
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

    /// One complete frame: input, simulation, render, present, telemetry.
    #[allow(clippy::too_many_lines)] // one cohesive frame: input, update, draw
    fn frame(&mut self) {
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
            self.settings.set_quality(level);
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
        if self.game.frame_count() == 1 {
            perf::startup_mark("input responsive");
        }

        if self.load_intent.is_some() && self.game.app_state() == AppState::Playing {
            self.game.set_app_state(AppState::Paused);
            self.input_handler.clear_gameplay_inputs();
            self.game.reset_timing();
        }
        // Update player movement (only active during AppState::Playing)
        self.game
            .update_player_movement(self.input_handler.state_mut(), self.settings);
        self.log_player_state();
        // One press is one interaction: consume the edge the movement update
        // latched, resolve the object under the crosshair and run its
        // map-authored actions. `window_focused` gates the key the same way it
        // gates relative mouse mode.
        let interact_pressed = self.game.take_interact_press();
        if interact_pressed && self.window_focused {
            self.dispatch_interaction();
        }
        self.dispatch_dev_interactions();
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
        self.apply_world_commands();
        // Advance the dynamic objects (the demonstration drum and any other
        // spawned object) once per frame: transform only, never a geometry or
        // lightmap rebuild.
        self.renderer.update_dynamic(self.game.delta_seconds());
        // Animated characters follow the player's locomotion state unless a
        // map-authored route or interaction addresses them by instance id
        // (`game.entity_frames()`); the renderer re-skins only the characters
        // whose pose moved. A one-shot cue that completed this frame is fed
        // back as an `animation_complete` event, which is how a sequence's
        // `wait_animation` step and an authored binding learn about it.
        let characters = self.renderer.update_characters(
            self.game.delta_seconds(),
            self.game.locomotion_snapshot(),
            self.game.entity_frames(),
        );
        for instance_id in &characters.finished {
            self.game
                .entities_mut()
                .notify_animation_complete(instance_id, "");
        }
        self.write_nav_debug(false);
        let frame_update_done = Instant::now();

        self.render_and_present(frame_begin, frame_update_done);
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
                crate::logging::warn_once(
                    format!("interact-override:{id}"),
                    format!("PLACES_INTERACT: `{id}` is not interactable in this level"),
                );
                continue;
            };
            if let Some(report) = self.game.world_mut().dispatch_interaction(Some(index)) {
                crate::logging::info(format_args!(
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
        let Some(report) = self.game.dispatch_interaction() else {
            return;
        };
        if report.player_reset {
            crate::logging::info("[interact] reset_to_start: returned to the authored spawn");
        } else if report.labels_toggled() > 0 {
            let verb = if report.labels_shown > 0 {
                "shown"
            } else {
                "hidden"
            };
            crate::logging::info(format!("[interact] label {verb}"));
        } else if report.animations_started > 0 {
            crate::logging::info("[interact] animation cue set");
        } else if report.spawned > 0 {
            crate::logging::info("[interact] entity spawned");
        } else if report.unsupported > 0 {
            crate::logging::warn(
                "[interact] action is not implemented; validation should have rejected it",
            );
        } else if report.missing_targets > 0 {
            crate::logging::warn("[interact] action target names no placed instance");
        }
    }

    /// Applies the entity runtime's render and audio commands.
    ///
    /// The world never names a GPU object: it emits typed commands keyed by a
    /// stable runtime key, and this is the one place they become renderer
    /// calls. A failed spawn is counted in the world and reported once per
    /// model here, never retried in a loop.
    #[allow(clippy::too_many_lines)] // one command dispatcher, one arm per command
    fn apply_world_commands(&mut self) {
        let commands = self.game.entities_mut().take_commands();
        for command in commands {
            match command {
                crate::entities::WorldCommand::SpawnDynamic {
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
                    if let Some(instance_id) = instance_id.as_deref() {
                        match self.renderer.spawn_runtime_character(
                            instance_id,
                            &model,
                            position.to_array(),
                            yaw_degrees,
                            scale,
                        ) {
                            Ok(()) => spawned_character = true,
                            Err(reason) => {
                                crate::logging::info(format_args!(
                                    "[characters] `{model}` spawns through the dynamic path: \
                                     {reason}"
                                ));
                            }
                        }
                    }
                    if !spawned_character
                        && !self.renderer.spawn_runtime_model(
                            entity,
                            &model,
                            position.to_array(),
                            yaw_degrees,
                            scale,
                        )
                    {
                        self.game.entities_mut().note_spawn_failure();
                        crate::logging::warn_once(
                            format!("runtime-spawn:{model}"),
                            format!(
                                "[entities] runtime spawn of `{model}` failed; the entity exists \
                                 but draws nothing"
                            ),
                        );
                    }
                }
                crate::entities::WorldCommand::DespawnDynamic {
                    entity,
                    instance_id,
                } => {
                    self.renderer.despawn_runtime_model(entity);
                    if !instance_id.is_empty() {
                        self.renderer.despawn_runtime_character(&instance_id);
                    }
                }
                crate::entities::WorldCommand::SetDynamicTransform {
                    entity,
                    position,
                    yaw_degrees,
                } => {
                    self.renderer
                        .set_runtime_transform(entity, position.to_array(), yaw_degrees);
                    if let Some(instance_id) = self
                        .game
                        .entities()
                        .handle_for_dynamic_key(entity)
                        .and_then(|handle| self.game.entities().instance_id_of(handle))
                    {
                        self.renderer.set_runtime_character_transform(
                            instance_id,
                            position.to_array(),
                            yaw_degrees,
                        );
                    }
                }
                crate::entities::WorldCommand::SetDynamicEmission { entity, scale } => {
                    self.renderer.set_runtime_emission(entity, scale);
                }
                crate::entities::WorldCommand::PlaySound {
                    entity,
                    sound,
                    gain,
                    looped,
                } => {
                    crate::logging::warn_once(
                        "audio-unavailable",
                        format!(
                            "[audio] `{sound}` requested (entity {entity}, gain {gain}, \
                             looped {looped}); this build has no audio device backend, so \
                             nothing plays"
                        ),
                    );
                }
                crate::entities::WorldCommand::StopSound { entity } => {
                    let _ = entity;
                }
                crate::entities::WorldCommand::SetEffectEnabled { index, enabled } => {
                    let index = usize::try_from(index).unwrap_or(usize::MAX);
                    self.renderer.set_effect_enabled(index, enabled);
                }
                crate::entities::WorldCommand::SetWaterEnabled { index, enabled } => {
                    let index = usize::try_from(index).unwrap_or(usize::MAX);
                    self.game.set_water_enabled(index, enabled);
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
            let interval = self
                .renderer
                .set_swap_interval(self.settings.vsync_enabled());
            self.bench.set_reported_swap_interval(interval);
        }
        if apply.graphics {
            self.rebuild_graphics_resources();
        }
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
        }
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
                self.window.minimize();
                Ok(())
            }
            bench::WindowAction::Restore => {
                self.window.restore();
                Ok(())
            }
        };
        if let Err(error) = result {
            crate::logging::warn_once(
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

    /// Applies every changed graphics setting as one transaction.
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
    fn rebuild_graphics_resources(&mut self) {
        // The transition starts here: the harness measures settings changes
        // from this mark to the completion mark (the immediate apply below, or
        // the world commit when a preparation is required).
        self.trace.record(
            "settings_change",
            self.load_generation,
            &serde_json::json!({
                "quality": self.settings.quality_level().name(),
                "lightmaps": self.settings.lightmap_quality().name(),
                "reflections": self.settings.reflection_quality().name(),
                "bloom": self.settings.bloom_enabled(),
                "filtering": self.settings.texture_filtering_preset(),
            })
            .to_string(),
        );
        self.renderer.set_quality(self.settings.quality_level());
        self.renderer
            .set_lightmap_quality(self.settings.lightmap_quality());
        self.renderer
            .set_reflection_quality(self.settings.reflection_quality());
        self.renderer
            .set_bloom_enabled(self.settings.bloom_enabled());
        self.renderer
            .set_texture_filtering(self.settings.texture_filtering_preset());
        if self.load_intent.is_none() && self.renderer.apply_frame_graphics() {
            self.applied_graphics_settings = self.settings.clone();
            self.trace
                .record("settings_applied", self.load_generation, "immediate");
            return;
        }
        let source = self.load_source.clone().or_else(|| {
            self.current_level
                .as_ref()
                .map(|level| loading::Source::retained(level.clone()))
        });
        let Some(source) = source else {
            return;
        };
        let intent = self.load_intent.unwrap_or(LoadIntent::Graphics);
        self.request_load(source, intent);
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

    /// `PLACES_STATE_LOG=file.csv`: append the player state every few frames.
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

    /// Reconciles SDL's relative mouse mode with the game state.
    ///
    /// Relative mode is on exactly while gameplay is active and the window has
    /// focus: every menu, the pause screen and a focus loss release the cursor,
    /// and regaining focus while Playing captures it again. The cursor is never
    /// warped. SDL is polled for the real state rather than caching what the
    /// last call asked for: a platform can drop relative mode on its own (a
    /// focus change, an OS capture change), and the mode is re-asserted on the
    /// next frame whenever it differs from the desired one.
    fn sync_mouse_capture(&self) {
        let desired = self.load_intent.is_none()
            && self.game.app_state() == AppState::Playing
            && self.window_focused;
        if self.sdl.mouse().relative_mouse_mode(self.window) != desired {
            self.sdl
                .mouse()
                .set_relative_mouse_mode(self.window, desired);
        }
    }

    fn handle_scripted_action(&mut self, received: perf::actions::Received) -> bool {
        self.trace.record(
            "action",
            self.load_generation,
            &serde_json::json!({
                "id": received.id, "kind": format!("{:?}", received.action),
                "latency_ms": received.latency.as_secs_f64() * 1000.0,
            })
            .to_string(),
        );
        match received.action {
            perf::actions::Action::Load { level } => {
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
            perf::actions::Action::Escape {} => {
                self.handle_event(&Event::KeyDown {
                    timestamp: 0,
                    window_id: self.window.id(),
                    keycode: Some(Keycode::Escape),
                    scancode: None,
                    keymod: sdl3::keyboard::Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                });
                self.handle_event(&Event::KeyUp {
                    timestamp: 0,
                    window_id: self.window.id(),
                    keycode: Some(Keycode::Escape),
                    scancode: None,
                    keymod: sdl3::keyboard::Mod::NOMOD,
                    repeat: false,
                    which: 0,
                    raw: 0,
                });
            }
            perf::actions::Action::Resize { width, height } => {
                if let Err(error) = self.window.set_size(width, height) {
                    self.fatal_error = Some(error.to_string());
                    self.game.stop();
                }
            }
            perf::actions::Action::Quality { level } => {
                if let Some(quality) = quality::QualityLevel::parse(&level) {
                    self.settings.set_quality(quality);
                }
            }
            perf::actions::Action::Lightmaps { quality } => {
                if let Some(quality) = quality::LightmapQuality::parse(&quality) {
                    self.settings.set_lightmap_quality(quality);
                }
            }
            perf::actions::Action::Focus { focused } => {
                return self.handle_event(&Event::Window {
                    timestamp: 0,
                    window_id: self.window.id(),
                    win_event: if focused {
                        WindowEvent::FocusGained
                    } else {
                        WindowEvent::FocusLost
                    },
                });
            }
            perf::actions::Action::Quit {} => {
                return self.handle_event(&Event::Quit { timestamp: 0 });
            }
        }
        self.game.is_running()
    }

    /// Handles one SDL event; returns `false` when the pump should stop.
    fn handle_event(&mut self, event: &Event) -> bool {
        if let Some(received) = self
            .actions
            .as_ref()
            .and_then(|actions| actions.receive(event))
        {
            return self.handle_scripted_action(received);
        }

        if let Event::Quit { .. } = event {
            self.game.stop();
            return false;
        }

        // Window focus is tracked before any screen-specific handling so a
        // focus change is never swallowed by a rebind or a menu branch:
        // `sync_mouse_capture` reconciles relative mouse mode from it.
        if let Event::Window { win_event, .. } = event {
            if matches!(win_event, WindowEvent::FocusLost) {
                self.window_focused = false;
                // A key released while another window had focus may never
                // deliver its KeyUp. Releasing everything keeps a held Jump,
                // Crouch or Interact from surviving the refocus.
                self.input_handler.clear_gameplay_inputs();
            } else if matches!(win_event, WindowEvent::FocusGained) {
                self.window_focused = true;
            }
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
                    ..
                }
            )
        {
            self.cancel_loading();
            return true;
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
                // Every newly opened settings screen starts with the Advanced
                // group collapsed; the state is never persisted.
                self.ui_state.advanced_expanded = false;
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

    /// Queues a level without blocking event dispatch.
    fn load_level_at(&mut self, index: usize) {
        if let Some(entry) = self.level_manager.get_entry(index).cloned() {
            self.request_load(loading::Source::entry(entry), LoadIntent::Play);
        }
    }

    fn request_load(&mut self, source: loading::Source, intent: LoadIntent) {
        // An outstanding preparation for the same world and effective lightmap
        // quality is promoted instead of restarted: the startup background
        // world must survive the player selecting that same level, and a
        // partially uploaded install must not be torn down to redo identical
        // work. A superseded request for different content still replaces it.
        if self.reuse_outstanding_preparation(&source, intent) {
            return;
        }
        let level_id = source.level_id().to_string();
        let lightmaps = self.settings.lightmap_quality();
        let request = loading::Request {
            source: source.clone(),
            lightmaps,
        };
        match self.loader.request(request) {
            Ok(id) => {
                if self.load_intent.is_none() {
                    self.load_return_state = self.game.app_state();
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
                self.input_handler.clear_gameplay_inputs();
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

    /// True when the request already being prepared covers `source` exactly.
    ///
    /// A completed-but-uploading install may only be reused while no GPU
    /// setting changed since it was captured: the install uploads with the
    /// configuration it recorded when it was created, so a changed quality,
    /// reflection, bloom or filtering setting must issue a fresh request.
    fn reuse_outstanding_preparation(
        &mut self,
        source: &loading::Source,
        intent: LoadIntent,
    ) -> bool {
        if self.load_intent.is_none() || self.load_lightmaps != self.settings.lightmap_quality() {
            return false;
        }
        if self.pending_commit.is_some() && self.gpu_settings_changed() {
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

    /// True when an installed or uploading world's GPU configuration no longer
    /// matches the current settings.
    fn gpu_settings_changed(&self) -> bool {
        self.settings.quality_level() != self.applied_graphics_settings.quality_level()
            || self.settings.reflection_quality()
                != self.applied_graphics_settings.reflection_quality()
            || self.settings.bloom_enabled() != self.applied_graphics_settings.bloom_enabled()
            || self.settings.texture_filtering_preset()
                != self.applied_graphics_settings.texture_filtering_preset()
    }

    fn restore_applied_graphics(&mut self) {
        if self.current_level.is_none() {
            return;
        }
        let previous = &self.applied_graphics_settings;
        self.settings.quality.clone_from(&previous.quality);
        self.settings
            .texture_filtering
            .clone_from(&previous.texture_filtering);
        self.settings.lightmaps.clone_from(&previous.lightmaps);
        self.settings.reflections.clone_from(&previous.reflections);
        self.settings.bloom = previous.bloom;
        self.settings.overrides.quality = previous.overrides.quality;
        self.settings.overrides.lightmaps = previous.overrides.lightmaps;
        self.settings.overrides.reflections = previous.overrides.reflections;
        self.settings.overrides.bloom = previous.overrides.bloom;
        self.settings.pending.graphics = false;
        self.renderer.set_quality(self.settings.quality_level());
        self.renderer
            .set_lightmap_quality(self.settings.lightmap_quality());
        self.renderer
            .set_reflection_quality(self.settings.reflection_quality());
        self.renderer
            .set_bloom_enabled(self.settings.bloom_enabled());
        self.renderer
            .set_texture_filtering(self.settings.texture_filtering_preset());
        let _ = self.renderer.apply_frame_graphics();
        self.save_settings();
    }

    fn cancel_loading(&mut self) {
        // Before the first world exists, the outstanding preparation is the
        // only thing that can produce a usable background; cancelling it would
        // strand the process on an empty menu with nothing left to wait for.
        if self.startup != StartupPhase::Ready {
            return;
        }
        self.loader.cancel();
        self.trace
            .record("cancel_disposal_begin", self.load_generation, "");
        self.renderer.cancel_prepared_install();
        self.pending_commit = None;
        self.trace
            .record("cancel_disposal_end", self.load_generation, "");
        self.restore_applied_graphics();
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
                    if prepared.lightmaps != self.settings.lightmap_quality() {
                        if let Some(source) = self.load_source.clone() {
                            self.request_load(source, intent);
                        }
                        return;
                    }
                    self.trace.record("preparation_result", id, &serde_json::json!({
                        "level": prepared.loaded.level.id, "wall_ms": prepared.preparation_millis,
                        "cache_hit": prepared.cache_hit,
                        "lighting_ms": if prepared.cache_hit { 0.0 } else { prepared.build.timings.lighting_millis },
                        "props_ms": if prepared.cache_hit { 0.0 } else { prepared.build.timings.props_millis },
                        "surfaces_ms": if prepared.cache_hit { 0.0 } else { prepared.build.timings.surfaces_millis },
                        "atlas_ms": if prepared.cache_hit { 0.0 } else { prepared.build.lightmap_millis },
                        "retained_bytes": prepared.build.retained_bytes(),
                    }).to_string());
                    let loading::PreparedWorld {
                        loaded,
                        build,
                        collision,
                        characters,
                        assets,
                        probes,
                        ..
                    } = prepared;
                    self.renderer.install_prepared_precompiled(
                        &loaded,
                        build,
                        assets,
                        characters,
                        matches!(intent, LoadIntent::Graphics),
                        probes,
                    );
                    self.pending_commit = Some(PendingCommit {
                        id,
                        loaded,
                        collision,
                        intent,
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
        let phase = self.loader.phase();
        if phase != self.load_phase {
            self.load_phase = phase;
            if let Some(phase) = phase {
                let label = match phase {
                    loading::Phase::Queued => "Waiting for previous preparation",
                    loading::Phase::Reading => "Reading level and assets",
                    loading::Phase::Geometry => "Assembling compiled world",
                    loading::Phase::Lightmaps => "Decoding lightmaps",
                    loading::Phase::Collision => "Decoding collision",
                    loading::Phase::Characters => "Preparing characters",
                    loading::Phase::Ready => "Uploading level",
                };
                self.ui_state
                    .set_status(format!("{label}... Esc to cancel"), false);
                self.trace.record("phase", self.load_generation, label);
            }
        }
    }

    /// Handles one failed preparation: restores the last good configuration
    /// and either retries the essential first world or returns to a screen.
    fn recover_from_preparation_failure(&mut self, id: u64, error: &str) {
        if let (Some(actions), Some(source)) = (&self.actions, &self.load_source)
            && let Err(notify_error) = actions.notify(&format!("failed:{}", source.level_id()))
        {
            self.fatal_error = Some(notify_error);
            self.game.stop();
        }
        self.restore_applied_graphics();
        self.load_intent = None;
        self.load_source = None;
        self.trace_world("load_recovered");
        self.trace.record("failed", id, error);
        self.ui_state
            .set_status(format!("Could not load level: {error}"), true);
        // The initial world is what gives the menu its background, so a
        // failure here retries the default level once rather than leaving no
        // preparation and a black menu.
        if self.startup != StartupPhase::Ready {
            if self.startup == StartupPhase::Preparing {
                self.startup = StartupPhase::Retrying;
                crate::logging::warn(format!(
                    "initial level preparation failed; retrying the demo: {error}"
                ));
                self.request_load(loading::Source::Default, LoadIntent::Background);
                return;
            }
            self.startup = StartupPhase::Ready;
        }
        self.game.set_app_state(self.load_return_state);
        self.input_handler.clear_gameplay_inputs();
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

    fn trace_world(&mut self, event: &str) {
        let (renderer_level, renderer_quality, renderer_lightmaps) =
            self.renderer.installed_identity();
        let position = self.game.player_position.to_array();
        self.trace.record(event, self.load_generation, &serde_json::json!({
            "current_level_id": self.current_level.as_ref().map(|loaded| loaded.level.id.as_str()),
            "renderer_level_id": renderer_level,
            "walls": self.game.walls().len(), "interactables": self.game.interactables().len(),
            "routes": self.game.routes().len(), "triggers": self.game.volume_count(),
            "characters": self.renderer.character_count(), "dynamic_objects": self.renderer.dynamic_scene().len(),
            "player_position": position, "quality": self.settings.quality_level().name(),
            "lightmaps": self.settings.lightmap_quality().name(), "reflections": self.settings.reflection_quality().name(),
            "renderer_quality": renderer_quality.name(), "renderer_lightmaps": renderer_lightmaps.name(),
            "bloom": self.settings.bloom_enabled(), "window_focused": self.window_focused,
        }).to_string());
    }

    fn commit_loaded_world(&mut self) {
        let Some(PendingCommit {
            id,
            loaded,
            collision,
            intent,
        }) = self.pending_commit.take()
        else {
            return;
        };
        self.trace.record("gpu_ready", id, &loaded.level.id);
        logging::info(format!("[loading] committed {}", loaded.level.id));
        self.load_intent = None;
        if matches!(intent, LoadIntent::Graphics) {
            self.game.set_app_state(self.load_return_state);
        } else {
            spawn_level_demonstration(self.renderer, &loaded);
            *self.spawn_pos = game::spawn_position(&loaded.level);
            *self.spawn_yaw = loaded.level.spawn.yaw_degrees.to_radians();
            self.game
                .reset_level(*self.spawn_pos, *self.spawn_yaw, collision);
            if matches!(intent, LoadIntent::Play) {
                self.game.set_app_state(AppState::Playing);
            }
            self.ready_frames = 0;
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
        self.applied_graphics_settings = self.settings.clone();
        log_prop_usage(self.renderer);
        *self.current_level = Some(loaded);
        // The first committed world is the menu background; the normal menu
        // may be revealed from now on. It is also the fallback a later
        // replacement can be cancelled back to.
        self.startup = StartupPhase::Ready;
        self.load_source = None;
        self.load_phase = None;
        self.ui_state.clear_status();
        self.input_handler.clear_gameplay_inputs();
        self.game.reset_timing();
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
        self.trace_world("world_committed");
        self.trace.record("ready", id, "");
        self.write_nav_debug(true);
    }

    /// Writes the developer navigation/AI inspection files when
    /// `PLACES_NAV_DEBUG` names a directory.
    ///
    /// The dump is generated from the live [`crate::nav::NavMesh`] and
    /// [`crate::ai::AiWorld`] the game is querying, not from a separate model,
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
                // Every newly opened settings screen starts with the Advanced
                // group collapsed; the state is never persisted.
                self.ui_state.advanced_expanded = false;
                self.goto(AppState::PauseSettings);
            }
            2 => self.goto(AppState::MainMenu),
            _ => {}
        }
    }

    /// Submits the UI pass: cached menu/settings geometry, the debug overlay
    /// when visible, and — while playing — the world-anchored interaction
    /// labels and the aimed-at prompt through the same text pipeline.
    fn submit_ui(
        &mut self,
        drawable: DrawableSize,
        cam_pos: Vec3,
        cam_yaw: f32,
        cam_pitch: f32,
        skip_render: bool,
    ) {
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
        let ui_vertices = self.ui_cache.get(
            self.game.app_state(),
            self.ui_state,
            self.settings,
            self.display_status,
            APP_VERSION,
        );
        // Labels are only appended while actually playing, never in a menu or a
        // pause; the overlay is appended on demand.
        let show_labels = self.game.app_state() == AppState::Playing;
        if show_labels || self.perf_overlay.is_visible() {
            self.ui_scratch.clear();
            self.ui_scratch.extend_from_slice(ui_vertices);
            if show_labels {
                interact::append_world_labels(
                    self.ui_scratch,
                    self.game,
                    &render::RenderCamera::new(
                        cam_pos,
                        cam_yaw,
                        cam_pitch,
                        self.settings.fov_degrees,
                    ),
                    drawable,
                );
            }
            if self.perf_overlay.is_visible() {
                self.ui_scratch
                    .extend_from_slice(self.perf_overlay.cached_vertices());
            }
            self.renderer.render_ui(self.ui_scratch);
        } else {
            self.renderer.render_ui(ui_vertices);
        }
    }

    /// Takes the one-shot `PLACES_CAPTURE` frame when it is due.
    ///
    /// The capture waits for a ready world so it records the finished screen,
    /// never a preparation frame.
    fn capture_if_due(&mut self, ready: bool) {
        if !ready
            || !self
                .actions
                .as_ref()
                .is_none_or(perf::actions::Actions::complete)
            || self.ready_frames < self.capture_at_frame
        {
            return;
        }
        let Some(path) = self.capture_path.take() else {
            return;
        };
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
        // `PLACES_CAMERA=yaw[,pitch]` pins the camera so a hardware benchmark
        // measures the same view twice; it never changes gameplay.
        let (cam_yaw, cam_pitch) = match self.bench.camera_override() {
            Some((yaw, pitch)) => (yaw.to_radians(), pitch.to_radians()),
            None => (cam_yaw, cam_pitch),
        };

        let skip_render = self.bench.skip_render();
        if !skip_render {
            self.renderer.render_scene(render::RenderCamera::new(
                cam_pos,
                cam_yaw,
                cam_pitch,
                self.settings.fov_degrees,
            ));
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
        self.submit_ui(drawable, cam_pos, cam_yaw, cam_pitch, skip_render);
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
        if ready {
            self.ready_frames = self.ready_frames.saturating_add(1);
        }
        self.capture_if_due(ready);

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
            self.bench.record_frame(
                frame_begin,
                frame_update_done,
                frame_render_done,
                frame_ui_done,
                frame_swap_done,
                self.renderer.render_stats(),
            );
            if self.bench.is_complete()
                && self
                    .actions
                    .as_ref()
                    .is_none_or(perf::actions::Actions::complete)
            {
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
        "[settings] quality {} (saved {}){} | bloom {} | reflections {}{} | lightmaps {}{} | vsync {} | filtering {} | window {} {}",
        settings.quality_level().name(),
        settings.quality,
        if settings.quality_overridden() {
            " [startup override]"
        } else {
            ""
        },
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

/// Boots SDL, points relative paths at the running installation, loads the
/// persisted settings (with any startup overrides) and creates the window.
fn bootstrap() -> Result<(Sdl, VideoSubsystem, Window, Settings, Bench), String> {
    let package = use_package_assets();
    log_package(&package);
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
    let mut settings = Settings::load_or_default();
    settings.apply_startup_overrides(bench.vsync_override());
    settings.ensure_saved();
    log_effective_settings(&settings);
    perf::startup_mark("settings");

    let (sdl_context, video_subsystem, window) = create_sdl_and_window(&mut settings)?;
    perf::startup_mark("window");
    // Boot applies every setting explicitly (window, swap interval, renderer),
    // so no pending work is owed after it.
    let _ = settings.take_pending_apply();
    Ok((sdl_context, video_subsystem, window, settings, bench))
}

fn initial_level(level_manager: &loader::LevelManager) -> (Option<loader::LevelEntry>, bool) {
    let requested = std::env::var("PLACES_LEVEL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let initial_entry = requested
        .as_ref()
        .and_then(|requested| {
            level_manager.entries().iter().find(|entry| {
                entry.id == requested.trim() || entry.name.eq_ignore_ascii_case(requested.trim())
            })
        })
        .or_else(|| {
            level_manager
                .entries()
                .iter()
                .find(|entry| entry.id == loader::DEMO_LEVEL_ID)
        })
        .cloned();
    let direct = requested.as_ref().is_some_and(|requested| {
        initial_entry.as_ref().is_some_and(|entry| {
            entry.id == requested.trim() || entry.name.eq_ignore_ascii_case(requested.trim())
        })
    });
    if requested.is_some() && !direct {
        logging::warn("Requested level was not found; loading Places Demo");
    }
    (initial_entry, direct)
}

/// Runs the Places player: window, renderer, discovery, loading and the frame loop.
///
/// The library root exists so the offline compiler (`places-compile`) shares the
/// engine instead of duplicating it; the player binary is a thin wrapper.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // The geometry checker is a headless CLI mode: it must run before any SDL
    // or wgpu bootstrap, and it exits the process with its own status.
    let args: Vec<String> = std::env::args().skip(1).collect();
    match geometry_check::options_from_args(&args) {
        Ok(Some(options)) => return geometry_check::main(&options),
        Ok(None) => {}
        Err(error) => geometry_check::exit_usage(&error),
    }
    match geometry_check::repair_options_from_args(&args) {
        Ok(Some(options)) => return geometry_check::repair_main(&options),
        Ok(None) => {}
        Err(error) => geometry_check::exit_usage(&error),
    }
    let mut trace = perf::loading::LoadTrace::new();
    trace.record("entry", 0, "");
    perf::startup_begin();
    let (sdl_context, video_subsystem, mut window, mut settings, mut bench) = bootstrap()?;

    let mut level_manager = loader::LevelManager::new();
    log_asset_catalog(&level_manager);
    perf::startup_mark("asset catalog");
    let mut renderer = create_renderer(&window, &settings, &bench)?;
    configure_vsync(&mut renderer, &mut bench, &settings);
    trace.record("window_ready", 0, "");

    let mut event_pump = sdl_context
        .event_pump()
        .map_err(|e| format!("Failed to init event pump: {e}"))?;

    let action_events = sdl_context.event().map_err(|error| error.to_string())?;
    let actions = perf::actions::Actions::from_env(&action_events, window.id())?;
    let mut input_handler = InputHandler::new();
    let mut spawn_pos = Vec3::ZERO;
    let mut spawn_yaw = 0.0;
    let mut game = Game::new(spawn_pos, spawn_yaw, CollisionWorld::default());
    let mut current_level = None;
    let mut display_status = DisplayStatus::default();
    let (initial_entry, direct) = initial_level(&level_manager);
    let loader = loading::Loader::new(loader::LevelManager::new())?;
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

    let applied_graphics_settings = settings.clone();
    let initial_lightmaps = settings.lightmap_quality();
    let mut frame_loop = FrameLoop {
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
        window_focused: true,
        fatal_error: None,
        loader,
        load_intent: None,
        pending_commit: None,
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
        applied_graphics_settings,
        graphics_hint: None,
        nav_debug_dir: std::env::var_os("PLACES_NAV_DEBUG").map(PathBuf::from),
        nav_debug_age: 0,
        dev_interactions: interact_overrides_from_env(),
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

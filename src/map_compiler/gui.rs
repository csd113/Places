//! Small native front end for the same compiler used by `build`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::engine::quality::LightmapQuality;
use crate::map_compiler::progress::{Session, Snapshot, WorkCount};
use crate::map_compiler::{self as compiler, BuildReport, BuildRequest};
use sdl3::dialog::{DialogError, DialogFileFilter};
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use sdl3::mouse::MouseButton;
use sdl3::pixels::{Color, PixelFormat};
use sdl3::render::{BlendMode, FRect, ScaleMode, Texture, WindowCanvas};

const WIDTH: u32 = 860;
const HEIGHT: u32 = 560;
const INK: Color = Color::RGB(226, 233, 240);
const MUTED: Color = Color::RGB(156, 173, 190);
const ACCENT: Color = Color::RGB(95, 205, 174);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Source,
    Output,
    Bake,
    Cancel,
}

impl Action {
    const ALL: [Self; 4] = [Self::Source, Self::Output, Self::Bake, Self::Cancel];

    fn rect(self) -> FRect {
        match self {
            Self::Source => FRect::new(644.0, 100.0, 184.0, 40.0),
            Self::Output => FRect::new(644.0, 184.0, 184.0, 40.0),
            Self::Bake => FRect::new(32.0, 266.0, 220.0, 44.0),
            Self::Cancel => FRect::new(268.0, 266.0, 152.0, 44.0),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Source => "Choose map...",
            Self::Output => "Save as...",
            Self::Bake => "Bake lighting",
            Self::Cancel => "Cancel",
        }
    }

    fn contains(self, x: f32, y: f32) -> bool {
        let rect = self.rect();
        x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h
    }
}

type Selection = (Action, Result<Vec<PathBuf>, DialogError>);

struct App {
    source: Option<PathBuf>,
    output: Option<PathBuf>,
    asset_root: PathBuf,
    workers: usize,
    session: Session,
    job: Option<JoinHandle<Result<BuildReport, String>>>,
    dialog_open: bool,
    closing: bool,
    focus: Action,
    message: String,
    snapshot: Option<Snapshot>,
    successful: bool,
}

impl App {
    fn enabled(&self, action: Action) -> bool {
        if self.dialog_open || self.closing {
            return false;
        }
        match action {
            Action::Source => self.job.is_none(),
            Action::Output | Action::Bake => self.job.is_none() && self.source.is_some(),
            Action::Cancel => self.job.is_some() && !self.session.interrupted(),
        }
    }

    fn select_source(&mut self, path: PathBuf) {
        self.output = Some(compiler::package_path_for(&path));
        self.source = Some(path);
        self.snapshot = None;
        self.successful = false;
        self.message = "Ready. Bakes Static + Full lighting and reflection probes.".into();
    }

    fn bake(&mut self) -> Result<(), String> {
        let source = self.source.as_ref().ok_or("Choose a source map first")?;
        let output = self
            .output
            .as_ref()
            .ok_or("Choose an output package first")?;
        validate_paths(source, output)?;
        self.session.reset_cancellation();
        self.snapshot = None;
        self.successful = false;
        let request = BuildRequest {
            source: source.clone(),
            out: output.clone(),
            asset_root: self.asset_root.clone(),
            variants: LightmapQuality::ALL.to_vec(),
            workers: self.workers,
            force: true,
            capture_probes: true,
        };
        self.job = Some(
            std::thread::Builder::new()
                .name("compiler-bake".into())
                .spawn(move || compiler::build(&request))
                .map_err(|error| format!("Could not start bake: {error}"))?,
        );
        self.message = "Baking Static + Full lighting and reflection probes...".into();
        self.focus = Action::Cancel;
        Ok(())
    }

    fn poll(&mut self) {
        if self.job.is_none() {
            return;
        }
        if let Some(snapshot) = self.session.snapshot() {
            self.snapshot = Some(snapshot);
        }
        if self.session.interrupted() {
            self.message =
                "Stopping at a safe boundary; waiting for the current operation...".into();
        }
        if !self.job.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        let Some(job) = self.job.take() else {
            return;
        };
        let result = job
            .join()
            .unwrap_or_else(|_| Err("The compiler worker stopped unexpectedly".into()));
        self.message = match result {
            Ok(report) => {
                self.successful = true;
                if report.warnings.is_empty() {
                    format!(
                        "Bake complete. Saved {} ({} bytes).",
                        report.out, report.bytes
                    )
                } else {
                    format!(
                        "Package saved to {}. Notes: {}",
                        report.out,
                        report.warnings.join("; ")
                    )
                }
            }
            Err(_) if self.session.interrupted() => {
                "Bake cancelled. You can start another bake.".into()
            }
            Err(error) => format!("Bake failed: {error}"),
        };
        crate::engine::logging::warn(&self.message);
        self.focus = Action::Bake;
    }

    fn activate(
        &mut self,
        action: Action,
        canvas: &WindowCanvas,
        sender: &Sender<Selection>,
    ) -> Result<(), String> {
        if !self.enabled(action) {
            return Ok(());
        }
        match action {
            Action::Bake => self.bake(),
            Action::Cancel => {
                self.session.cancel();
                Ok(())
            }
            Action::Source | Action::Output => {
                let responses = sender.clone();
                let callback: sdl3::dialog::DialogCallback = Box::new(move |result, _filter| {
                    let _sent = responses.send((action, result));
                });
                let result = if action == Action::Source {
                    sdl3::dialog::show_open_file_dialog(
                        &[DialogFileFilter {
                            name: "Unbaked Places map",
                            pattern: "json",
                        }],
                        self.source.as_deref(),
                        false,
                        canvas.window(),
                        callback,
                    )
                } else {
                    sdl3::dialog::show_save_file_dialog(
                        &[DialogFileFilter {
                            name: "Baked Places package",
                            pattern: "placesmap",
                        }],
                        self.output.as_deref(),
                        canvas.window(),
                        callback,
                    )
                };
                result.map_err(|error| format!("Could not open file picker: {error}"))?;
                self.dialog_open = true;
                Ok(())
            }
        }
    }

    fn selections(&mut self, receiver: &Receiver<Selection>) {
        while let Ok((action, result)) = receiver.try_recv() {
            self.dialog_open = false;
            match result {
                Ok(paths) => {
                    if let Some(path) = paths.into_iter().next() {
                        match action {
                            Action::Source => self.select_source(path),
                            Action::Output => {
                                self.output = Some(package_selection(path));
                                self.successful = false;
                                self.message =
                                    "Ready. Click Bake lighting to write the selected package."
                                        .into();
                            }
                            Action::Bake | Action::Cancel => {}
                        }
                    }
                }
                Err(DialogError::Canceled) => {}
                Err(error) => self.message = format!("File picker failed: {error}"),
            }
        }
    }

    fn close(&mut self) {
        self.closing = true;
        if self.job.is_some() {
            self.session.cancel();
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Even a rendering error must not detach a worker still publishing a package.
        if let Some(job) = self.job.take() {
            self.session.cancel();
            let _joined = job.join();
        }
    }
}

fn validate_paths(source: &Path, output: &Path) -> Result<(), String> {
    if source
        .extension()
        .is_none_or(|extension| extension != "json")
        || !source.is_file()
    {
        return Err("Select an existing unbaked .json map".into());
    }
    if output
        .extension()
        .is_none_or(|extension| extension != "placesmap")
    {
        return Err("The output must have a .placesmap extension".into());
    }
    if source.canonicalize().ok() == output.canonicalize().ok() {
        return Err("The output must not overwrite the source map".into());
    }
    Ok(())
}

fn package_selection(path: PathBuf) -> PathBuf {
    // Native save dialogs do not consistently append the filter's extension.
    if path.extension().is_none() {
        path.with_extension("placesmap")
    } else {
        path
    }
}

/// Opens the native Map Compiler window and processes jobs until it closes.
///
/// # Errors
/// Returns an error when SDL, the window, font texture or progress session
/// cannot be initialized, or when a frame cannot be drawn.
pub fn run(
    source: Option<PathBuf>,
    output: Option<PathBuf>,
    asset_root: PathBuf,
    workers: usize,
) -> Result<(), String> {
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let video = sdl.video().map_err(|error| error.to_string())?;
    let window = video
        .window("Places - Map Compiler", WIDTH, HEIGHT)
        .position_centered()
        .build()
        .map_err(|error| error.to_string())?;
    let mut canvas = window.into_canvas();
    let textures = canvas.texture_creator();
    let (font_width, font_height) = crate::engine::font::font_atlas_dimensions();
    let mut font = textures
        .create_texture_static(PixelFormat::RGBA32, font_width, font_height)
        .map_err(|error| error.to_string())?;
    font.update(None, &crate::engine::font::generate_font_atlas(), 1024)
        .map_err(|error| error.to_string())?;
    font.set_blend_mode(BlendMode::Blend);
    font.set_scale_mode(ScaleMode::Nearest);
    let mut events = sdl.event_pump().map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::channel();
    let mut app = App {
        source: None,
        output: None,
        asset_root,
        workers,
        session: Session::start_observed()?,
        job: None,
        dialog_open: false,
        closing: false,
        focus: Action::Source,
        message: "Choose an unbaked .json map to begin.".into(),
        snapshot: None,
        successful: false,
    };
    if let Some(path) = source {
        app.select_source(path);
    }
    if output.is_some() {
        app.output = output;
    }
    loop {
        for event in events.poll_iter() {
            if let Some(action) = handle_event(&event, &mut app)
                && let Err(error) = app.activate(action, &canvas, &sender)
            {
                app.message = error;
            }
        }
        app.selections(&receiver);
        app.poll();
        if app.closing && app.job.is_none() {
            break;
        }
        draw(&mut canvas, &mut font, &app).map_err(|error| error.to_string())?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn handle_event(event: &Event, app: &mut App) -> Option<Action> {
    if matches!(
        event,
        Event::Quit { timestamp: _ }
            | Event::Window {
                timestamp: _,
                window_id: _,
                win_event: WindowEvent::CloseRequested,
            }
    ) {
        app.close();
    }
    if let Event::MouseButtonUp {
        timestamp: _,
        window_id: _,
        which: _,
        clicks: _,
        mouse_btn: MouseButton::Left,
        x,
        y,
    } = event
    {
        return Action::ALL
            .into_iter()
            .find(|action| action.contains(*x, *y));
    }
    if let Event::KeyDown {
        timestamp: _,
        window_id: _,
        scancode: _,
        keymod: _,
        which: _,
        raw: _,
        keycode: Some(key),
        repeat: false,
    } = event
    {
        if *key == Keycode::Tab {
            let next = Action::ALL
                .into_iter()
                .cycle()
                .skip_while(|action| *action != app.focus)
                .skip(1)
                .take(4)
                .find(|action| app.enabled(*action));
            if let Some(focus) = next {
                app.focus = focus;
            }
        } else if matches!(*key, Keycode::Return | Keycode::Space) {
            return Some(app.focus);
        } else if *key == Keycode::Escape {
            return Some(Action::Cancel);
        }
    }
    None
}

fn text(
    canvas: &mut WindowCanvas,
    font: &mut Texture<'_>,
    value: &str,
    x: f32,
    y: f32,
    color: Color,
) -> Result<(), sdl3::Error> {
    font.set_color_mod(color.r, color.g, color.b);
    let mut cursor = x;
    for character in value.chars() {
        // The atlas reserves the space cell as a solid white UI swatch.
        if character == ' ' {
            cursor += 12.0;
            continue;
        }
        let Some([u, v, _, _]) = crate::engine::font::get_char_uv(character)
            .or_else(|| crate::engine::font::get_char_uv('?'))
        else {
            continue;
        };
        let src = FRect::new(u * 256.0, v * 128.0, 8.0, 8.0);
        canvas.copy(font, src, FRect::new(cursor, y, 12.0, 12.0))?;
        cursor += 12.0;
    }
    Ok(())
}

fn lines(
    canvas: &mut WindowCanvas,
    font: &mut Texture<'_>,
    value: &str,
    y: f32,
    limit: usize,
    color: Color,
) -> Result<(), sdl3::Error> {
    let chars: Vec<char> = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut top = y;
    for chunk in chars.chunks(65).take(limit) {
        text(
            canvas,
            font,
            &chunk.iter().collect::<String>(),
            32.0,
            top,
            color,
        )?;
        top += 18.0;
    }
    Ok(())
}

fn path_label(selected: Option<&Path>) -> String {
    let Some(path) = selected else {
        return "No file selected".into();
    };
    let value = path.to_string_lossy();
    if value.chars().count() <= 49 {
        return value.into_owned();
    }
    let suffix: String = value
        .chars()
        .rev()
        .take(46)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{suffix}")
}

fn progress_tenths(work: &WorkCount) -> u16 {
    u16::try_from(
        u128::try_from(work.completed.min(work.total))
            .unwrap_or(0)
            .saturating_mul(1000)
            .checked_div(u128::try_from(work.total).unwrap_or(0))
            .unwrap_or(0),
    )
    .unwrap_or(1000)
}

fn draw(canvas: &mut WindowCanvas, font: &mut Texture<'_>, app: &App) -> Result<(), sdl3::Error> {
    canvas.set_draw_color(Color::RGB(22, 29, 38));
    canvas.clear();
    text(canvas, font, "PLACES / MAP COMPILER", 32.0, 28.0, ACCENT)?;
    text(canvas, font, "Source map (.json)", 32.0, 76.0, MUTED)?;
    text(
        canvas,
        font,
        &path_label(app.source.as_deref()),
        32.0,
        114.0,
        INK,
    )?;
    text(
        canvas,
        font,
        "Baked package (.placesmap)",
        32.0,
        160.0,
        MUTED,
    )?;
    text(
        canvas,
        font,
        &path_label(app.output.as_deref()),
        32.0,
        198.0,
        INK,
    )?;
    for action in Action::ALL {
        let rect = action.rect();
        let enabled = app.enabled(action);
        canvas.set_draw_color(if enabled {
            Color::RGB(44, 63, 77)
        } else {
            Color::RGB(31, 40, 50)
        });
        canvas.fill_rect(rect)?;
        if enabled && app.focus == action {
            canvas.set_draw_color(ACCENT);
            canvas.draw_rect(rect)?;
        }
        text(
            canvas,
            font,
            action.label(),
            rect.x + 12.0,
            rect.y + 14.0,
            if enabled { INK } else { MUTED },
        )?;
    }
    draw_progress(canvas, font, app)?;
    let _presented = canvas.present();
    Ok(())
}

fn draw_progress(
    canvas: &mut WindowCanvas,
    font: &mut Texture<'_>,
    app: &App,
) -> Result<(), sdl3::Error> {
    let active = app.job.is_some();
    let snapshot = app
        .snapshot
        .as_ref()
        .filter(|_| active && !app.session.interrupted());
    let stage = snapshot.map_or_else(
        || {
            if active {
                app.message.clone()
            } else {
                if app.successful {
                    "Bake complete"
                } else {
                    "Ready"
                }
                .into()
            }
        },
        |view| {
            format!(
                "{} {}  |  {}:{:02}",
                view.variant,
                view.stage,
                view.elapsed.as_secs() / 60,
                view.elapsed.as_secs() % 60
            )
        },
    );
    lines(canvas, font, &stage, 342.0, 2, INK)?;
    canvas.set_draw_color(Color::RGB(42, 54, 67));
    canvas.fill_rect(FRect::new(32.0, 390.0, 796.0, 20.0))?;
    let work = snapshot.and_then(|view| view.work.as_ref());
    let fill = if app.successful && !active {
        796.0
    } else {
        work.map_or(0.0, |count| f32::from(progress_tenths(count)) * 0.796)
    };
    canvas.set_draw_color(ACCENT);
    if fill > 0.0 {
        canvas.fill_rect(FRect::new(32.0, 390.0, fill, 20.0))?;
    }
    let caption = work.map_or_else(
        || {
            if active {
                "Working (this stage has no item count)"
            } else {
                "Percentages show the current pass, not the entire bake."
            }
            .into()
        },
        |count| {
            format!(
                "{} / {} {} ({}%)",
                count.completed,
                count.total,
                count.unit,
                progress_tenths(count) / 10
            )
        },
    );
    text(canvas, font, &caption, 32.0, 422.0, MUTED)?;
    lines(
        canvas,
        font,
        &app.message,
        458.0,
        4,
        if app.successful { ACCENT } else { INK },
    )?;
    Ok(())
}

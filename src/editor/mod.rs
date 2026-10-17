//! Native basic authoring application. Engine data, geometry and rendering are
//! shared; documents, history, panels and child-process lifetimes belong here.

mod actions;
pub mod document;
mod input;
pub mod process;
mod ui;
mod verification;
mod viewport;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::engine::{assets, logging, paths};
use document::{Document, ObjectId};
use process::{CompileJob, PlayJob, ToolPaths};
use sdl3::dialog::DialogError;
use viewport::Viewport;

use actions::Popup;

type DialogResult = (bool, Result<Vec<PathBuf>, DialogError>);

struct ViewOptions {
    grid: bool,
    snap: bool,
}

struct Capture {
    directory: PathBuf,
    index: u16,
    pending: bool,
}

impl Capture {
    fn new(directory: PathBuf) -> Result<Self, String> {
        if !directory.is_dir() {
            return Err("Capture directory must already exist".into());
        }
        Ok(Self {
            directory,
            index: 0,
            pending: false,
        })
    }
}

struct App {
    capture: Option<Capture>,
    document: Option<Document>,
    viewport: Viewport,
    tools: ToolPaths,
    catalog: assets::AssetCatalog,
    compile: Option<CompileJob>,
    play: Option<PlayJob>,
    play_after_build: bool,
    selected: Option<ObjectId>,
    status: String,
    view: ViewOptions,
    step: f32,
    object_scroll: usize,
    prop: String,
    popup: Option<Popup>,
    dialog_open: bool,
    dialog_sender: Sender<DialogResult>,
    dialog_receiver: Receiver<DialogResult>,
    closing: bool,
}

impl App {
    fn load(&mut self, path: &Path) -> Result<(), String> {
        let document = Document::open(path)?;
        self.viewport.rebuild(document.level());
        self.viewport.frame_next_preview();
        self.document = Some(document);
        self.selected = None;
        self.viewport.select(None);
        self.object_scroll = 0;
        self.status = format!("Opened {}", path.display());
        Ok(())
    }

    fn changed(&mut self) {
        if let Some(document) = &self.document {
            self.viewport.rebuild(document.level());
        }
        self.viewport.select(self.selected);
    }

    fn report(&mut self, result: Result<(), String>) {
        if let Err(error) = result {
            logging::warn(&error);
            self.status = error;
        }
    }

    fn save(&mut self) -> Result<(), String> {
        if self.compile.is_some() {
            return Err("Wait for the current build or cancel it before saving again. New edits remain in memory.".into());
        }
        let document = self.document.as_mut().ok_or("Open a project first")?;
        document.validate_assets(&self.catalog)?;
        document.save_all()?;
        self.status = format!("Saved {}", document.root().display());
        Ok(())
    }

    fn load_initial(
        &mut self,
        project: Option<PathBuf>,
        new: bool,
        level: Option<String>,
    ) -> Result<(), String> {
        if let Some(path) = project {
            if new {
                self.create_project(&path)?;
            } else {
                self.load(&path)?;
            }
            if let Some(selected_level) = level {
                if let Some(document) = &mut self.document {
                    document.select_level(&selected_level)?;
                }
                self.changed();
            }
        }

        Ok(())
    }

    fn capture_frame(&mut self) {
        if self.viewport.is_preparing() {
            return;
        }
        if let Some(capture) = &mut self.capture
            && capture.pending
        {
            capture.pending = false;
            capture.index = capture.index.saturating_add(1);
            let path = capture
                .directory
                .join(format!("editor-{:03}.png", capture.index));
            let result = self.viewport.capture(&path);
            self.report(result);
        }
    }

    fn dialogs(&mut self) {
        while let Ok((new, selection)) = self.dialog_receiver.try_recv() {
            self.dialog_open = false;
            match selection {
                Ok(paths) => {
                    if let Some(path) = paths.first() {
                        let result = if new {
                            self.create_project(&actions::new_project_path(path))
                        } else {
                            self.load(path)
                        };
                        self.report(result);
                    }
                }
                Err(error) => self.status = format!("File picker: {error}"),
            }
        }
    }
}

#[derive(Default)]
struct Options {
    project: Option<PathBuf>,
    new: bool,
    level: Option<String>,
    compiler: Option<PathBuf>,
    game: Option<PathBuf>,
    asset_root: Option<PathBuf>,
    capture_dir: Option<PathBuf>,
    verify_script: Option<PathBuf>,
}

fn options() -> Result<Option<Options>, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                use std::io::Write;
                let _written = writeln!(
                    std::io::stdout(),
                    "places-editor [PROJECT] [--new PROJECT] [--level ID] [--compiler PATH] [--game PATH] [--asset-root PATH]\nNative project editor. Built-in assets only. See docs/EDITOR.md for controls."
                );
                return Ok(None);
            }
            "--new" => {
                options.new = true;
                options.project = Some(PathBuf::from(
                    args.next().ok_or("--new needs a new project directory")?,
                ));
            }
            "--level" => {
                options.level = Some(args.next().ok_or("--level needs an exact level ID")?);
            }
            "--compiler" => {
                options.compiler =
                    Some(PathBuf::from(args.next().ok_or("--compiler needs a path")?));
            }
            "--game" => {
                options.game = Some(PathBuf::from(args.next().ok_or("--game needs a path")?));
            }
            "--verify-script" => {
                options.verify_script = Some(PathBuf::from(
                    args.next()
                        .ok_or("--verify-script needs a native verification script")?,
                ));
            }
            "--capture-dir" => {
                options.capture_dir = Some(PathBuf::from(
                    args.next()
                        .ok_or("--capture-dir needs an existing directory")?,
                ));
            }
            "--asset-root" => {
                options.asset_root = Some(PathBuf::from(
                    args.next().ok_or("--asset-root needs a path")?,
                ));
            }
            _ if !arg.starts_with('-') && options.project.is_none() => {
                options.project = Some(PathBuf::from(arg));
            }
            _ => return Err(format!("Unknown editor argument: {arg}")),
        }
    }
    Ok(Some(options))
}

fn application(options: Options) -> Result<(), String> {
    let mut verification = options
        .verify_script
        .as_deref()
        .map(verification::Verification::from_path)
        .transpose()?;
    if let Some(root) = options.asset_root {
        paths::configure_assets(&root)?;
    }
    assets::validate_installation()?;
    let _user = paths::initialize_user()?;
    let mut tools = ToolPaths::discover()?;
    if let Some(path) = options.compiler {
        tools.set_compiler(&path)?;
    }
    if let Some(path) = options.game {
        tools.set_game(&path)?;
    }
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let mut window = video
        .window("Places - Editor", 1440, 816)
        .position_centered()
        .resizable()
        .build()
        .map_err(|e| e.to_string())?;
    let _minimum = window.set_minimum_size(960, 544);
    let viewport = Viewport::new(&window)?;
    let (dialog_sender, dialog_receiver) = mpsc::channel();
    let capture = options.capture_dir.map(Capture::new).transpose()?;
    let mut app = App {
        capture,
        document: None,
        viewport,
        tools,
        catalog: assets::AssetCatalog::load_default(),
        compile: None,
        play: None,
        play_after_build: false,
        selected: None,
        status: "New: choose a parent folder. Open: choose an existing project folder.".into(),
        view: ViewOptions {
            grid: true,
            snap: true,
        },
        step: 0.25,
        object_scroll: 0,
        prop: "core:chair".into(),
        popup: None,
        dialog_open: false,
        dialog_sender,
        dialog_receiver,
        closing: false,
    };
    app.load_initial(options.project, options.new, options.level)?;
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    while !app.closing {
        let (_, hits) = ui::build(&app);
        for event in events.poll_iter() {
            app.event(&event, &window, &hits);
        }
        if app.popup.is_some() || app.dialog_open {
            app.viewport.cancel_navigation();
        }
        app.dialogs();
        app.poll_jobs();
        if let Some(error) = app.viewport.status() {
            app.status = format!("Viewport: {error}");
        }
        let wants_text = matches!(app.popup, Some(Popup::Number { field: _, value: _ }));
        let text_input = video.text_input();
        if text_input.is_active(&window) != wants_text {
            if wants_text {
                text_input.start(&window);
            } else {
                text_input.stop(&window);
            }
        }
        let title = app.document.as_ref().map_or_else(
            || "Places - Editor".into(),
            |d| {
                format!(
                    "{}{} - Places Editor",
                    d.project().manifest().name,
                    if d.is_dirty() { " *" } else { "" }
                )
            },
        );
        let _title = window.set_title(&title);
        let (vertices, _hits) = ui::build(&app);
        let _presented = app.viewport.draw(&window, &vertices);
        app.capture_frame();
        if let Some(driver) = &mut verification {
            driver.advance(&mut app, &window)?;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    Ok(())
}

/// Starts the separate native editor application.
#[must_use]
pub fn run() -> std::process::ExitCode {
    match options().and_then(|value| value.map_or(Ok(()), application)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            logging::warn(format!("Editor: {error}"));
            std::process::ExitCode::FAILURE
        }
    }
}

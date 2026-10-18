//! Native basic authoring application. Engine data, geometry and rendering are
//! shared; documents, history, panels and child-process lifetimes belong here.

mod actions;
mod audio_preview;
mod authoring;
mod component_actions;
pub mod document;
mod gameplay_actions;
mod geometry_actions;
mod input;
mod navigation;
pub mod process;
mod projects;
mod safety;
mod safety_actions;
mod scripting;
mod selection;
mod transform;
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

enum DialogPurpose {
    Project(bool),
    Thumbnail,
}
type DialogResult = (DialogPurpose, Result<Vec<PathBuf>, DialogError>);

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
    project_operation: Option<projects::Operation>,
    project_import: Option<PathBuf>,
    selected: Option<ObjectId>,
    status: String,
    ui_focus: Option<ui::Action>,
    thumbnail_image: Option<std::sync::Arc<crate::engine::materials::RawImage>>,
    view: ViewOptions,
    navigation: navigation::Inspection,
    step: f32,
    object_scroll: usize,
    object_query: String,
    prop: String,
    sound_cues: Vec<String>,
    preview_audio: crate::engine::audio::AudioSystem,
    sound_preview_until: Option<std::time::Instant>,
    popup: Option<Popup>,
    script_feedback: String,
    component_selection: component_actions::Selection,
    safety_report: Option<safety::SafetyReport>,
    safety_job: Option<Receiver<(serde_json::Value, safety::SafetyReport)>>,
    safety_index: usize,
    opening_rect: Option<[f32; 4]>,
    dialog_open: bool,
    dialog_sender: Sender<DialogResult>,
    dialog_receiver: Receiver<DialogResult>,
    closing: bool,
}

impl App {
    fn load(&mut self, path: &Path) -> Result<(), String> {
        let document = Document::open(path)?;
        self.invalidate_navigation();
        self.viewport.rebuild(document.level());
        self.viewport.frame_next_preview();
        self.document = Some(document);
        self.safety_report = None;
        self.safety_index = 0;
        self.opening_rect = None;
        self.viewport.highlight(None, &[]);
        self.selected = None;
        self.viewport.select(None);
        self.object_scroll = 0;
        self.object_query.clear();
        self.status = format!("Opened {}", path.display());
        Ok(())
    }

    fn changed(&mut self) {
        self.invalidate_navigation();
        self.safety_report = None;
        self.safety_index = 0;
        self.opening_rect = None;
        self.viewport.highlight(None, &[]);
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
        self.status = format!("Saved {}", document.manifest().name);
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

    fn update_window(&self, window: &mut sdl3::video::Window, video: &sdl3::VideoSubsystem) {
        let wants_text = matches!(
            self.popup,
            Some(
                Popup::Number { field: _, value: _ }
                    | Popup::ProjectText { field: _, value: _ }
                    | Popup::ScriptEdit {
                        path: _,
                        cursor: _,
                        scroll: _,
                        all: _
                    }
                    | Popup::Transform(_)
                    | Popup::AuthoringForm(_)
                    | Popup::SceneSearch(_)
            )
        );
        let text_input = video.text_input();
        if text_input.is_active(window) != wants_text {
            if wants_text {
                text_input.start(window);
            } else {
                text_input.stop(window);
            }
        }
        let title = self.document.as_ref().map_or_else(
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
    }

    fn draw(&mut self, window: &sdl3::video::Window) {
        let (vertices, _hits) = ui::build(self);
        let images = if matches!(self.popup, Some(Popup::Thumbnail)) {
            self.thumbnail_image
                .iter()
                .map(|image| crate::engine::render::UiImage {
                    bounds: [112.0, 86.0, 256.0, 144.0],
                    image: std::sync::Arc::clone(image),
                })
                .collect()
        } else {
            Vec::new()
        };
        self.viewport.set_ui_images(images);
        let _presented = self.viewport.draw(window, &vertices);
    }

    fn dialogs(&mut self) {
        while let Ok((purpose, selection)) = self.dialog_receiver.try_recv() {
            self.dialog_open = false;
            if matches!(purpose, DialogPurpose::Thumbnail) {
                let result = selection
                    .map_err(|error| error.to_string())
                    .and_then(|paths| {
                        if let Some(path) = paths.first() {
                            self.document
                                .as_mut()
                                .ok_or("Open a project first")?
                                .attach_thumbnail(path)?;
                            self.refresh_thumbnail_preview();
                        }
                        Ok(())
                    });
                self.report(result);
                continue;
            }
            let new = matches!(purpose, DialogPurpose::Project(true));
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

fn configured_tools(options: &Options) -> Result<ToolPaths, String> {
    let mut tools = ToolPaths::discover()?;
    if let Some(path) = &options.compiler {
        tools.set_compiler(path)?;
    }
    if let Some(path) = &options.game {
        tools.set_game(path)?;
    }
    Ok(tools)
}

fn editor_window(video: &sdl3::VideoSubsystem) -> Result<sdl3::video::Window, String> {
    let mut window = video
        .window("Places - Editor", 1440, 816)
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .map_err(|error| error.to_string())?;
    let _minimum = window.set_minimum_size(960, 544);
    Ok(window)
}

fn application(options: Options) -> Result<(), String> {
    let mut verification = options
        .verify_script
        .as_deref()
        .map(verification::Verification::from_path)
        .transpose()?;
    if let Some(root) = &options.asset_root {
        paths::configure_assets(root)?;
    }
    assets::validate_installation()?;
    let _user = paths::initialize_user()?;
    let tools = configured_tools(&options)?;
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let video = sdl.video().map_err(|e| e.to_string())?;
    let mut window = editor_window(&video)?;
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
        project_operation: None,
        project_import: None,
        selected: None,
        ui_focus: None,
        thumbnail_image: None,
        status: "New: choose a parent folder. Open: choose an existing project folder.".into(),
        view: ViewOptions {
            grid: true,
            snap: true,
        },
        navigation: navigation::Inspection::default(),
        step: 0.25,
        object_scroll: 0,
        object_query: String::new(),
        prop: "core:chair".into(),
        sound_cues: crate::engine::audio::AudioCatalog::load(paths::assets()?)?
            .cue_ids()
            .map(str::to_string)
            .collect(),
        preview_audio: crate::engine::audio::AudioSystem::new(paths::assets()?, &sdl),
        sound_preview_until: None,
        popup: None,
        script_feedback: String::new(),
        component_selection: component_actions::Selection::default(),
        safety_report: None,
        safety_job: None,
        safety_index: 0,
        opening_rect: None,
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
        if (app.popup.is_some()
            && !matches!(
                app.popup,
                Some(Popup::Safety {
                    index: _,
                    preview: true
                })
            ))
            || app.dialog_open
        {
            app.viewport.cancel_navigation();
        }
        app.dialogs();
        app.update_audio_preview();
        app.poll_jobs();
        app.poll_safety();
        app.poll_navigation();
        if let Some(error) = app.viewport.status() {
            app.status = format!("Viewport: {error}");
        }
        app.update_window(&mut window, &video);
        app.draw(&window);
        app.capture_frame();
        if let Some(driver) = &mut verification {
            driver.advance(&mut app, &mut window)?;
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

//! Editor commands; each document mutation enters the same undo history.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::paths;
use sdl3::video::Window;

use super::App;
use super::document::{Document, ObjectKind};
use super::process::{BuildOutcome, CompileJob, PlayJob, PlayOutcome};
use super::ui::{self, Action, Hit};

#[derive(Clone, Copy)]
pub(super) enum Pending {
    New,
    Open,
    Close,
}

pub(super) enum Popup {
    Layers,
    Flicker,
    Template,
    Behavior,
    Stairs(Box<super::stairs::Wizard>),
    DoorAssets,
    Bakes,
    Authoring,
    AuthoringForm(Box<super::authoring::Draft>),
    SceneSearch(String),
    Transform(Box<super::transform::Draft>),
    Project,
    Thumbnail,
    ProjectText {
        field: super::projects::Field,
        value: String,
    },
    Scripts,
    ScriptEdit {
        path: String,
        cursor: usize,
        scroll: usize,
        all: bool,
    },
    ScriptHandlers {
        path: String,
        names: Vec<String>,
        page: usize,
    },
    ScriptDiagnostics,
    Unsaved(Pending),
    Number {
        field: usize,
        value: String,
    },
    Library {
        materials: bool,
        page: usize,
    },
    Geometry,
    Create,
    Gameplay {
        page: usize,
    },
    GameplayActions,
    Components {
        page: usize,
    },
    ComponentText {
        field: super::document::components::Field,
        value: String,
        page: usize,
    },
    GameplayChoices {
        choice: super::gameplay_actions::GameplayChoice,
        page: usize,
    },
    Shape,
    Safety {
        index: usize,
        preview: bool,
    },
}

impl Popup {
    pub fn draw(&self, app: &App, v: &mut Vec<crate::engine::render::Vertex>, hits: &mut Vec<Hit>) {
        hits.clear();
        if matches!(self, Self::Flicker) {
            super::flicker::draw(app, v, hits);
            return;
        }
        if matches!(self, Self::Template) {
            super::templates::draw(app, v, hits);
            return;
        }
        if matches!(self, Self::Behavior) {
            super::behavior::draw(app, v, hits);
            return;
        }
        if let Self::Stairs(wizard) = self {
            super::stairs::draw(app, v, hits, wizard);
            return;
        }
        if matches!(self, Self::Bakes) {
            super::bakes::draw(app, v, hits);
            return;
        }
        if matches!(self, Self::Authoring | Self::AuthoringForm(_)) {
            super::authoring::draw(app, v, hits);
            return;
        }
        if let Self::Transform(draft) = self {
            ui::transform_popup(draft, v, hits);
            return;
        }
        if matches!(
            self,
            Self::Project | Self::Thumbnail | Self::ProjectText { field: _, value: _ }
        ) {
            super::projects::draw(app, v, hits);
            return;
        }
        if matches!(
            self,
            Self::Scripts
                | Self::ScriptEdit {
                    path: _,
                    cursor: _,
                    scroll: _,
                    all: _
                }
                | Self::ScriptHandlers {
                    path: _,
                    names: _,
                    page: _
                }
                | Self::ScriptDiagnostics
        ) {
            ui::scripts_popup(app, v, hits);
            return;
        }
        if let Self::Safety { index, preview } = self {
            ui::safety_popup(app, v, hits, *index, *preview);
            return;
        }
        ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.6]);
        ui::rect(v, [130.0, 65.0, 260.0, 146.0], [0.075, 0.11, 0.14, 1.0]);
        self.draw_content(app, v, hits);
    }

    fn draw_content(
        &self,
        app: &App,
        v: &mut Vec<crate::engine::render::Vertex>,
        hits: &mut Vec<Hit>,
    ) {
        match self {
            Self::Layers => super::focus::draw(app, v, hits),
            Self::SceneSearch(value) => super::selection::draw_search(v, hits, value),
            Self::Flicker
            | Self::Template
            | Self::Behavior
            | Self::Stairs(_)
            | Self::Bakes
            | Self::Authoring
            | Self::AuthoringForm(_)
            | Self::Transform(_)
            | Self::Project
            | Self::Thumbnail
            | Self::ProjectText { field: _, value: _ }
            | Self::Scripts
            | Self::ScriptEdit {
                path: _,
                cursor: _,
                scroll: _,
                all: _,
            }
            | Self::ScriptHandlers {
                path: _,
                names: _,
                page: _,
            }
            | Self::ScriptDiagnostics
            | Self::Safety {
                index: _,
                preview: _,
            } => {}
            Self::Create => ui::create_popup(v, hits),
            Self::Gameplay { page } => ui::gameplay_popup(app, v, hits, *page),
            Self::GameplayActions => ui::action_popup(v, hits),
            Self::Components { page } => ui::components::draw(app, v, hits, *page),
            Self::ComponentText {
                field,
                value,
                page: _,
            } => {
                ui::components::draw_text(v, hits, *field, value);
            }
            Self::GameplayChoices { choice, page } => {
                ui::choices_popup(app, v, hits, *choice, *page);
            }
            Self::Geometry => ui::geometry_popup(app, v, hits),
            Self::Shape => ui::shape_popup(app, v, hits),
            Self::Unsaved(_) => {
                ui::text(
                    v,
                    "Save changes before continuing?",
                    140.0,
                    80.0,
                    ui::INK,
                    60,
                );
                ui::text(
                    v,
                    "All dirty levels are protected.",
                    140.0,
                    96.0,
                    ui::INK,
                    60,
                );
                for (x, label, action) in [
                    (140.0, "Save", Action::ConfirmSave),
                    (218.0, "Discard", Action::ConfirmDiscard),
                    (296.0, "Cancel", Action::ConfirmCancel),
                ] {
                    ui::popup_button(v, hits, [x, 130.0, 70.0, 15.0], label, action);
                }
            }
            Self::Number { value, field } => draw_number(app, v, value, *field),
            Self::DoorAssets => ui::library::draw_grid(app, v, hits, &app.door_choices(), 0, None),
            Self::Library { materials, page } => ui::library_popup(app, v, hits, *materials, *page),
        }
    }
}

fn draw_number(
    app: &App,
    vertices: &mut Vec<crate::engine::render::Vertex>,
    value: &str,
    field: usize,
) {
    ui::text(
        vertices,
        &format!("{} / Enter apply / Esc cancel", app.field_label(field)),
        140.0,
        80.0,
        ui::INK,
        65,
    );
    ui::text(vertices, value, 140.0, 104.0, [0.4, 0.9, 0.72, 1.0], 60);
    draw_numeric_hint(vertices);
}

pub(super) fn new_project_path(parent: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    parent.join(format!("untitled-{stamp}"))
}

impl App {
    pub(super) fn create_project(&mut self, path: &Path) -> Result<(), String> {
        let id = format!(
            "room_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let document = Document::create(path, &id, &name)?;
        self.document = Some(document);
        self.selected = None;
        self.object_scroll = 0;
        self.changed();
        self.viewport.frame_all();
        self.status = format!("Created {name}. Add geometry or props; click values to edit.");
        Ok(())
    }

    pub(super) fn request(&mut self, pending: Pending, window: &Window) {
        if self.compile.is_some() || self.play.is_some() {
            self.status =
                "Cancel the build or stop play before switching projects or closing.".into();
        } else if self.document.as_ref().is_some_and(Document::is_dirty) {
            self.popup = Some(Popup::Unsaved(pending));
        } else {
            self.perform(pending, window);
        }
    }

    fn perform(&mut self, pending: Pending, window: &Window) {
        match pending {
            Pending::Close => self.closing = true,
            Pending::New | Pending::Open => {
                if self.dialog_open {
                    return;
                }
                let new = matches!(pending, Pending::New);
                let sender = self.dialog_sender.clone();
                let start = self
                    .document
                    .as_ref()
                    .map(|document| document.root().to_path_buf())
                    .or_else(|| {
                        paths::user()
                            .ok()
                            .map(super::super::engine::paths::UserPaths::projects)
                    });
                sdl3::dialog::show_open_folder_dialog(
                    start,
                    false,
                    window,
                    Box::new(move |result, _filter| {
                        let _sent = sender.send((super::DialogPurpose::Project(new), result));
                    }),
                );
                self.dialog_open = true;
            }
        }
    }

    pub(super) fn library(&self, materials: bool) -> std::rc::Rc<[String]> {
        self.browser.choices(if materials {
            super::browser::ChoiceKind::Materials
        } else {
            super::browser::ChoiceKind::Models
        })
    }

    fn confirm_action(&mut self, action: Action, window: &Window) -> Result<(), String> {
        if let Some(Popup::Unsaved(pending)) = self.popup {
            if action == Action::ConfirmSave {
                self.save()?;
            }
            self.popup = None;
            self.perform(pending, window);
        }
        Ok(())
    }

    fn contextual_action(&mut self, action: Action, window: &Window) -> Result<bool, String> {
        Ok(self.focus_action(action)?
            || self.flicker_action(action)?
            || self.template_action(action)?
            || self.behavior_action(action)?
            || self.stair_action(action)?
            || self.authoring_action(action)?
            || self.project_action(action, window)?
            || self.script_action(action)?
            || self.navigation_action(action)?
            || self.build_action(action)?
            || self.geometry_action(action)?
            || self.safety_action(action)?
            || self.component_action(action)?
            || self.transform_action(action)?
            || self.selection_ui_action(action)?
            || self.gameplay_action(action)?)
    }

    pub(super) fn activate(&mut self, action: Action, window: &Window) -> Result<(), String> {
        if matches!(
            action,
            Action::Nudge(_, _)
                | Action::Field(_)
                | Action::Transform
                | Action::Delete
                | Action::Duplicate
                | Action::Materials
                | Action::Gameplay
                | Action::Components(_)
        ) && self.selected.is_some_and(|id| !self.editable(id))
        {
            return Err(
                "Selection is locked or hidden. Show and unlock it in Layers before editing."
                    .into(),
            );
        }
        if self.placement.is_some()
            && !matches!(
                action,
                Action::ChainWalls
                    | Action::ApplyPlacement
                    | Action::CancelPlacement
                    | Action::Snap
                    | Action::Step
                    | Action::Grid
            )
        {
            self.cancel_placement();
        }
        if self.contextual_action(action, window)? {
            return Ok(());
        }
        if !self.global_action(action, window)? {
            return Err("Unhandled editor command".into());
        }
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Global commands return unrelated contextual commands to the application dispatcher."
    )]
    fn global_action(&mut self, action: Action, window: &Window) -> Result<bool, String> {
        match action {
            Action::Place(tool) => self.start_placement(tool)?,
            Action::ApplyPlacement => self.apply_path_placement()?,
            Action::CancelPlacement => {
                self.cancel_placement();
                self.status = "Placement cancelled.".into();
            }
            Action::ChainWalls => {
                if let Some(p) = &mut self.placement {
                    p.chain = !p.chain;
                }
            }
            Action::ThumbnailSize => {
                self.browser.compact = !self.browser.compact;
                self.reset_browser_page();
            }
            Action::BakeStatus => self.check_bakes()?,
            Action::BakeLevel(index) => self.build_bake_row(index)?,
            Action::BakeCancel => self.bakes.invalidate(),
            Action::New => self.request(Pending::New, window),
            Action::Open => self.request(Pending::Open, window),
            Action::Save => self.save()?,
            Action::Capture => self.request_capture(),
            Action::Undo | Action::Redo => self.history_action(action == Action::Redo),
            Action::Add(kind) => self.add_safely(kind)?,
            Action::Delete | Action::Duplicate => {
                self.selection_action(action == Action::Duplicate)?;
            }
            Action::Focus | Action::Grid | Action::Mode | Action::Snap | Action::Step => {
                self.view_action(action);
            }
            Action::Nudge(field, positive) => self.nudge(field, positive)?,
            Action::DoorAssets => {
                self.browser.query.clear();
                self.popup = Some(Popup::DoorAssets);
            }
            Action::MaterialTarget => {
                self.browser.material_ceiling = !self.browser.material_ceiling;
            }
            Action::BrowserTheme => self.browser_theme(),
            Action::ChooseAsset(index) => self.choose_asset(index)?,
            Action::Page(forward) => self.page_library(forward),
            Action::ClosePopup | Action::ConfirmCancel => self.close_popup(),
            Action::ConfirmSave | Action::ConfirmDiscard => self.confirm_action(action, window)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn add_safely(&mut self, kind: ObjectKind) -> Result<(), String> {
        match kind {
            ObjectKind::Wall => return self.start_placement(super::construction::Tool::Wall),
            ObjectKind::Door => return self.start_placement(super::construction::Tool::Door),
            ObjectKind::Light => return self.start_placement(super::construction::Tool::Light),
            ObjectKind::Room
            | ObjectKind::Prop
            | ObjectKind::Ramp
            | ObjectKind::FloorRegion
            | ObjectKind::Audio
            | ObjectKind::Entity
            | ObjectKind::Trigger
            | ObjectKind::FogRegion
            | ObjectKind::GroundFog
            | ObjectKind::SpawnPoint => {}
        }
        if kind == ObjectKind::Room {
            let _opened = self.authoring_action(Action::Author(
                super::authoring::Command::Open(super::authoring::Tool::Room),
            ))?;
            let _created = self.authoring_action(Action::Author(super::authoring::Command::New))?;
            return Ok(());
        }
        if kind == ObjectKind::Ramp {
            let _opened = self.authoring_action(Action::Author(
                super::authoring::Command::Open(super::authoring::Tool::Ramp),
            ))?;
            let _created = self.authoring_action(Action::Author(super::authoring::Command::New))?;
            return Ok(());
        }
        if let Err(error) = self.add_object(kind) {
            self.selected = None;
            self.viewport.select(None);
            self.popup = None;
            return Err(format!(
                "Add refused: {error}. Selection cleared; existing objects are unchanged."
            ));
        }
        Ok(())
    }

    pub(super) fn open_number(&mut self, field: usize) -> Result<(), String> {
        let value = self.field(field)?;
        self.popup = Some(Popup::Number {
            field,
            value: format!("{value:.3}"),
        });
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only build and play commands belong to this handler."
    )]
    fn build_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Build | Action::Play => self.start_build(action == Action::Play)?,
            Action::Cancel => {
                self.play_after_build = false;
                if let Some(job) = &mut self.compile {
                    job.cancel();
                    self.status = "Cancelling safely...".into();
                }
            }
            Action::Stop => {
                if let Some(job) = &mut self.play {
                    job.stop();
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only viewport controls reach this contextual handler."
    )]
    fn view_action(&mut self, action: Action) {
        match action {
            Action::Focus => {
                if self.selected.is_some() {
                    self.viewport.frame_selected();
                } else {
                    self.viewport.frame_all();
                }
            }
            Action::Grid => {
                self.view.grid = !self.view.grid;
                self.viewport.set_grid(self.view.grid);
            }
            Action::Mode => {
                self.view.simple = !self.view.simple;
                self.ui_focus = None;
                self.status = if self.view.simple {
                    "Simple mode: choose an action, then select and place in the scene."
                } else {
                    "Advanced mode: all precise controls; the same scene and history."
                }
                .into();
            }
            Action::Snap => self.view.snap = !self.view.snap,
            Action::Step => {
                self.step = if self.step < 0.2 {
                    0.25
                } else if self.step < 0.4 {
                    0.5
                } else if self.step < 0.8 {
                    1.0
                } else {
                    0.1
                }
            }
            _ => {}
        }
    }

    fn history_action(&mut self, redo: bool) {
        self.focus.isolated = None;
        let reviewing = self.safety_report.is_some();
        if let Some(document) = &mut self.document {
            let changed = if redo {
                document.redo()
            } else {
                document.undo()
            };
            self.selected = None;
            self.changed();
            self.status = if changed {
                if redo {
                    "Redo applied."
                } else {
                    "Undo applied."
                }
            } else {
                "No history step available."
            }
            .into();
            if reviewing {
                let result = self.start_safety();
                self.report(result);
            }
        }
    }

    fn selection_action(&mut self, duplicate: bool) -> Result<(), String> {
        self.focus.isolated = None;
        let id = self.selected.ok_or("Select an object first")?;
        let document = self.document.as_mut().ok_or("Open a project first")?;
        self.selected = if duplicate {
            Some(document.duplicate(id)?)
        } else {
            document.delete(id)?;
            None
        };
        self.changed();
        Ok(())
    }

    fn request_capture(&mut self) {
        if let Some(capture) = &mut self.capture {
            capture.pending = true;
        } else {
            self.status = "Launch with --capture-dir DIR to enable F12 captures.".into();
        }
    }

    fn nudge(&mut self, field: usize, positive: bool) -> Result<(), String> {
        let value = self.field(field)?;
        let amount = if matches!(field, 6 | 11 | 12) {
            if matches!(self.selected, Some(super::document::ObjectId::Prop(_))) {
                15.0
            } else {
                90.0
            }
        } else {
            self.step
        };
        self.apply_field(field, value + if positive { amount } else { -amount })?;

        Ok(())
    }

    fn add_object(&mut self, kind: ObjectKind) -> Result<(), String> {
        if !matches!(
            kind,
            ObjectKind::Room
                | ObjectKind::Wall
                | ObjectKind::Prop
                | ObjectKind::Ramp
                | ObjectKind::FloorRegion
        ) {
            return self.add_gameplay(kind);
        }
        let mut target = self.viewport.target();
        target[1] = match kind {
            ObjectKind::Prop | ObjectKind::Ramp => 0.0,
            ObjectKind::FloorRegion => -1.25,
            ObjectKind::Light
            | ObjectKind::Audio
            | ObjectKind::Entity
            | ObjectKind::Door
            | ObjectKind::Trigger
            | ObjectKind::FogRegion
            | ObjectKind::GroundFog
            | ObjectKind::SpawnPoint
            | ObjectKind::Room
            | ObjectKind::Wall => self
                .document
                .as_ref()
                .and_then(|doc| {
                    crate::engine::level::LevelSurfaces::new(doc.level())
                        .floor_y_at(target[0], target[2])
                })
                .unwrap_or(0.0),
        };
        let position = target.map(|v| {
            if self.view.snap {
                (v / self.step).round() * self.step
            } else {
                v
            }
        });
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        self.selected = Some(doc.add_with_catalog(
            kind,
            position,
            if kind == ObjectKind::Prop {
                Some(&self.prop)
            } else {
                None
            },
            &self.catalog,
        )?);
        let mount_error = if kind == ObjectKind::Prop && self.view.snap {
            self.selected
                .and_then(|id| doc.mount_to_surface(id, Some(self.step)).err())
        } else {
            None
        };
        let label = match kind {
            ObjectKind::Room => "room",
            ObjectKind::Wall => "wall",
            ObjectKind::Prop => "prop",
            ObjectKind::Ramp => "ramp",
            ObjectKind::FloorRegion => "recess / basin",
            ObjectKind::Light
            | ObjectKind::Audio
            | ObjectKind::Entity
            | ObjectKind::Door
            | ObjectKind::Trigger
            | ObjectKind::FogRegion
            | ObjectKind::GroundFog
            | ObjectKind::SpawnPoint => "gameplay object",
        };
        self.status = format!("Added {label}. Click values or use +/- to transform.");
        if let Some(error) = mount_error {
            self.status = format!("Added {label}; adjust its mount: {error}");
        }
        self.popup = None;
        self.changed();

        Ok(())
    }

    fn choose_asset(&mut self, index: usize) -> Result<(), String> {
        if matches!(self.popup, Some(Popup::DoorAssets)) {
            let id = self
                .door_choices()
                .get(index)
                .cloned()
                .ok_or("Door selection expired")?;
            let style = id
                .strip_prefix("editor:door_")
                .ok_or("Invalid door style")?;
            self.document
                .as_mut()
                .ok_or("Open a project first")?
                .set_door_style(self.selected.ok_or("Select a door first")?, style)?;
            self.popup = None;
            self.changed();
            return Ok(());
        }

        if let Some(Popup::Library { materials, page: _ }) = self.popup {
            let id = self
                .library(materials)
                .get(index)
                .cloned()
                .ok_or("Asset selection expired")?;
            if materials {
                let selected = self.selected.ok_or("Select a room or wall")?;
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                if self.browser.material_ceiling {
                    doc.set_ceiling_material(selected, &id)?;
                } else {
                    doc.set_material(selected, &id)?;
                }
                self.changed();
                self.status = format!("Assigned material {id}");
            } else {
                if let Some(selected @ super::document::ObjectId::Prop(_)) = self.selected {
                    self.document
                        .as_mut()
                        .ok_or("Open a project first")?
                        .set_gameplay_asset(selected, &id, &self.catalog)?;
                    self.changed();
                    self.status = format!("Assigned model {id}; + Prop places another.");
                } else {
                    self.status = format!("Selected {id}; click + Prop to place it.");
                }
                self.prop = id;
            }
        }
        self.popup = None;

        Ok(())
    }

    pub(super) fn page_library(&mut self, forward: bool) {
        if matches!(self.popup, Some(Popup::Bakes)) {
            self.bakes.page = if forward {
                self.bakes
                    .page
                    .saturating_add(1)
                    .min(self.bakes.rows.len().saturating_sub(1) / 4)
            } else {
                self.bakes.page.saturating_sub(1)
            };
            return;
        }
        if let Some(Popup::ScriptHandlers {
            path: _,
            names,
            page,
        }) = &mut self.popup
        {
            *page = if forward {
                page.saturating_add(1)
                    .min(names.len().saturating_sub(1) / 20)
            } else {
                page.saturating_sub(1)
            };
            return;
        }
        if let Some(Popup::GameplayChoices { choice, page }) = self.popup {
            let count = self.gameplay_choices(choice).len();
            self.popup = Some(Popup::GameplayChoices {
                choice,
                page: if forward {
                    page.saturating_add(1).min(
                        count
                            .saturating_sub(1)
                            .checked_div(
                                if matches!(
                                    choice,
                                    super::gameplay_actions::GameplayChoice::Fixture
                                        | super::gameplay_actions::GameplayChoice::EntityModel
                                ) {
                                    self.browser_page_size()
                                } else {
                                    10
                                },
                            )
                            .unwrap_or(0),
                    )
                } else {
                    page.saturating_sub(1)
                },
            });
            return;
        }
        let count = if let Some(Popup::Library { materials, page: _ }) = self.popup {
            self.library(materials).len()
        } else {
            0
        };
        let page_size = self.browser_page_size();
        if let Some(Popup::Library { page, materials: _ }) = &mut self.popup {
            *page = if forward {
                page.saturating_add(1)
                    .min(count.saturating_sub(1).checked_div(page_size).unwrap_or(0))
            } else {
                page.saturating_sub(1)
            };
        }
    }

    pub(super) fn start_build(&mut self, play: bool) -> Result<(), String> {
        if self.compile.is_some() || self.play.is_some() {
            return Err("A build or player is already running".into());
        }
        self.save()?;
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        self.compile = Some(if play {
            let output = doc.root().join(format!("{}.placesmap", doc.manifest().id));
            CompileJob::operation(&self.tools, "export-project", doc.root(), &output)?
        } else {
            CompileJob::start(&self.tools, doc.project(), doc.level_id())?
        });
        self.play_after_build = play;
        self.status = "Compiling saved source...".into();

        Ok(())
    }

    const fn field_label(&self, field: usize) -> &'static str {
        match field {
            20..=23 => "Room edge displacement (m)",
            11 => "Pitch (degrees)",
            12 => "Roll (degrees)",
            100.. => "Gameplay property",
            7 => "Opening X (m)",
            8 => "Opening Z (m)",
            9 => "Opening width (m)",
            10 => "Opening depth (m)",
            1 if matches!(
                self.selected,
                Some(
                    super::document::ObjectId::Ramp(_) | super::document::ObjectId::FloorRegion(_)
                )
            ) =>
            {
                "Offset from room base (m)"
            }
            4 if matches!(self.selected, Some(super::document::ObjectId::Ramp(_))) => {
                "Signed ramp rise (m)"
            }
            6 => "Yaw / turn (degrees)",
            _ => "Transform value",
        }
    }

    pub(super) fn field(&self, field: usize) -> Result<f32, String> {
        if (20..=23).contains(&field) {
            return Ok(0.0);
        }
        if field >= 100 {
            return self.gameplay_number(field);
        }
        if (7..=10).contains(&field) {
            return self
                .opening_rect
                .as_ref()
                .and_then(|rect| rect.get(field.saturating_sub(7)))
                .copied()
                .ok_or_else(|| "Open room shape tools first".into());
        }
        let transform = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .transform(self.selected.ok_or("Select an object")?)?;
        match field {
            0..=2 => transform.position.get(field).copied(),
            3..=5 => transform.size.get(field.saturating_sub(3)).copied(),
            6 => Some(transform.yaw_degrees),
            11 => Some(transform.pitch_degrees),
            12 => Some(transform.roll_degrees),
            _ => None,
        }
        .ok_or_else(|| "Unknown transform field".into())
    }

    pub(super) fn apply_field(&mut self, field: usize, mut value: f32) -> Result<(), String> {
        if !value.is_finite() {
            return Err("Enter a finite number".into());
        }
        if field >= 100 {
            return self.apply_gameplay_number(field, value);
        }
        if let Some(edge) = field
            .checked_sub(20)
            .and_then(|index| super::document::room_resize::Edge::ALL.get(index))
            .copied()
        {
            self.document
                .as_mut()
                .ok_or("Open a project first")?
                .resize_room_edge(self.selected.ok_or("Select a room")?, edge, value)?;
            self.changed();
            self.status = "Room edge applied as one undo step.".into();
            return Ok(());
        }
        if self.view.snap && field < 6 {
            value = (value / self.step).round() * self.step;
        }
        if (7..=10).contains(&field) {
            let snapped = if self.view.snap {
                (value / self.step).round() * self.step
            } else {
                value
            };
            if field >= 9 && snapped <= 0.0 {
                return Err("Opening width and depth must be positive".into());
            }
            *self
                .opening_rect
                .as_mut()
                .and_then(|rect| rect.get_mut(field.saturating_sub(7)))
                .ok_or("Open room shape tools first")? = snapped;
            return Ok(());
        }
        let id = self.selected.ok_or("Select an object")?;
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let mut transform = doc.transform(id)?;
        match field {
            0..=2 => {
                *transform
                    .position
                    .get_mut(field)
                    .ok_or("Invalid position axis")? = value;
            }
            3..=5 => {
                *transform
                    .size
                    .get_mut(field.saturating_sub(3))
                    .ok_or("Invalid size axis")? = value;
            }
            6 => transform.yaw_degrees = value,
            11 => transform.pitch_degrees = value,
            12 => transform.roll_degrees = value,
            _ => return Err("Unknown transform field".into()),
        }
        if matches!(id, super::document::ObjectId::Pillar(_)) && matches!(field, 3 | 5) {
            transform.size[0] = value;
            transform.size[2] = value;
        }
        if matches!(id, super::document::ObjectId::Prop(_)) && (3..=5).contains(&field) {
            transform.size = [value; 3];
        }
        doc.set_transform(id, transform)?;
        self.changed();
        self.status = "Transform updated. Cmd/Ctrl Z to undo.".into();
        Ok(())
    }

    pub(super) fn poll_jobs(&mut self) {
        if let Some(job) = &mut self.compile {
            if job.cancelling() {
                self.status = "Cancelling at a safe compiler boundary...".into();
            } else if let Some(progress) = job.progress() {
                self.status = format!(
                    "{} / {} {}{}",
                    progress.source,
                    progress.variant,
                    progress.stage,
                    progress.work.as_ref().map_or_else(
                        || " ...".into(),
                        |work| format!(" {}/{} {}", work.completed, work.total, work.unit)
                    )
                );
            }
            if let Some(outcome) = job.poll() {
                let output = job.output().to_path_buf();
                self.compile = None;
                match outcome {
                    BuildOutcome::Succeeded(report) => {
                        if let Some(operation) = self.project_operation.take() {
                            match operation {
                                super::projects::Operation::Import => { let result = self.load(&output); self.report(result); },
                                super::projects::Operation::Check => self.status = "Project validated. Version 1 is supported; no conversion needed; no files changed.".into(),
                                super::projects::Operation::Export => self.status = format!("Exported complete project: {}", output.display()),
                            }
                        } else {
                            self.status = format!(
                                "Build succeeded: {} bytes. {}",
                                report.bytes,
                                report.warnings.join("; ")
                            );
                            if self.play_after_build {
                                let result = self
                                    .document
                                    .as_ref()
                                    .ok_or_else(|| "Project closed during build".into())
                                    .and_then(|doc| {
                                        PlayJob::start(&self.tools, &output, doc.level_id())
                                    });
                                match result {
                                    Ok(player) => {
                                        self.last_play_trace = Some(player.runtime_trace_path());
                                        self.play = Some(player);
                                        self.status =
                                            "Importing compiled level; player will open...".into();
                                    }
                                    Err(error) => self.status = error,
                                }
                            }
                        }
                    }
                    BuildOutcome::Failed(error) => {
                        self.project_operation = None;
                        crate::engine::logging::warn(&error);
                        self.status = format!("Build failed: {error}");
                    }
                    BuildOutcome::Cancelled => {
                        self.project_operation = None;
                        self.status =
                            "Build cancelled. Source and last published package retained.".into();
                    }
                }
                self.play_after_build = false;
            }
        }
        if let Some(job) = &mut self.play {
            let polled_play = job.poll();
            let messages: String = job
                .diagnostics()
                .lines()
                .skip_while(|line| !line.contains("[luau]"))
                .take(32)
                .collect::<Vec<_>>()
                .join("\n");
            if !messages.is_empty() {
                self.script_feedback = messages.chars().take(4096).collect();
            }
            if let Some(outcome) = polled_play {
                self.status = match outcome {
                    PlayOutcome::Returned => "Returned to editing.".into(),
                    PlayOutcome::Stopped => "Play stopped. Editor work retained.".into(),
                    PlayOutcome::Failed(error) => format!("Play failed: {error}"),
                };
                self.play = None;
            }
        }
    }
}

fn draw_numeric_hint(v: &mut Vec<crate::engine::render::Vertex>) {
    ui::text(
        v,
        "Position and size use the current snap step.",
        140.0,
        130.0,
        ui::INK,
        65,
    );
}

//! Editor commands; each document mutation enters the same undo history.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::engine::assets::AssetType;
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
        match self {
            Self::Project
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

    pub(super) fn library(&self, materials: bool) -> Vec<String> {
        self.catalog
            .entries()
            .into_iter()
            .filter(|entry| {
                if materials {
                    entry.asset_type.as_str() == AssetType::MATERIAL
                } else {
                    matches!(
                        entry.asset_type.as_str(),
                        AssetType::PROP | AssetType::ENTITY
                    )
                }
            })
            .map(|entry| entry.id.clone())
            .collect()
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
        Ok(self.project_action(action, window)?
            || self.script_action(action)?
            || self.navigation_action(action)?
            || self.build_action(action)?
            || self.geometry_action(action)?
            || self.safety_action(action)?
            || self.gameplay_action(action)?)
    }

    pub(super) fn activate(&mut self, action: Action, window: &Window) -> Result<(), String> {
        if self.contextual_action(action, window)? {
            return Ok(());
        }
        match action {
            Action::Project
            | Action::Navigation
            | Action::NavBake
            | Action::NavAgent(_)
            | Action::NavPick(_)
            | Action::NavUseAgent
            | Action::NavCoverage
            | Action::NavSlice(_)
            | Action::NavAllFloors
            | Action::NavClear
            | Action::Build
            | Action::Play
            | Action::Cancel
            | Action::Stop
            | Action::ProjectField(_)
            | Action::ProjectApply
            | Action::ProjectEntry
            | Action::MapCrosshair
            | Action::Thumbnail
            | Action::ThumbnailChoose
            | Action::ThumbnailRemove
            | Action::ProjectSelect(_)
            | Action::ProjectCheck
            | Action::ProjectMigrate
            | Action::Scripts
            | Action::ScriptNew
            | Action::ScriptOpen(_)
            | Action::ScriptDiagnostics
            | Action::ScriptCheck
            | Action::ScriptLink
            | Action::ScriptHandler(_)
            | Action::Create
            | Action::Gameplay
            | Action::GameplayPage(_)
            | Action::GameplayActions
            | Action::GamePick(_)
            | Action::GameChoose(_, _)
            | Action::GameSetting(_)
            | Action::RemoveTriggerStep(_)
            | Action::EnvironmentPreview
            | Action::MountSurface
            | Action::ToggleLightMount
            | Action::IgnoreFinding
            | Action::ClearFindingIgnore
            | Action::IgnoredFindings
            | Action::Geometry
            | Action::Shape
            | Action::CutOpening
            | Action::UpperFloor
            | Action::Safety
            | Action::Finding(_)
            | Action::PreviewFix
            | Action::ApplyFix
            | Action::InspectFinding
            | Action::CancelFix => {}
            Action::New => self.request(Pending::New, window),
            Action::Open => self.request(Pending::Open, window),
            Action::Save => self.save()?,
            Action::Capture => self.request_capture(),
            Action::Undo | Action::Redo => self.history_action(action == Action::Redo),
            Action::Add(kind) => self.add_object(kind)?,
            Action::Select(id) => {
                self.selected = Some(id);
                self.viewport.select(Some(id));
                self.viewport.highlight(None, &[]);
            }
            Action::Delete | Action::Duplicate => {
                self.selection_action(action == Action::Duplicate)?;
            }
            Action::Focus | Action::Grid | Action::Snap | Action::Step => self.view_action(action),
            Action::Field(field) => {
                let value = self.field(field)?;
                self.popup = Some(Popup::Number {
                    field,
                    value: format!("{value:.3}"),
                });
            }
            Action::Nudge(field, positive) => self.nudge(field, positive)?,
            Action::Assets | Action::Materials => {
                self.popup = Some(Popup::Library {
                    materials: action == Action::Materials,
                    page: 0,
                });
            }
            Action::ChooseAsset(index) => self.choose_asset(index)?,
            Action::Page(forward) => self.page_library(forward),
            Action::ClosePopup | Action::ConfirmCancel => self.close_popup(),
            Action::ConfirmSave | Action::ConfirmDiscard => self.confirm_action(action, window)?,
        }
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
        let reviewing = self.safety_report.is_some();
        if let Some(document) = &mut self.document {
            let _changed = if redo {
                document.redo()
            } else {
                document.undo()
            };
            self.selected = None;
            self.changed();
            if reviewing {
                let result = self.start_safety();
                self.report(result);
            }
        }
    }

    fn selection_action(&mut self, duplicate: bool) -> Result<(), String> {
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
            ObjectKind::Room => "floor",
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
        if let Some(Popup::Library { materials, page: _ }) = self.popup {
            let id = self
                .library(materials)
                .get(index)
                .cloned()
                .ok_or("Asset selection expired")?;
            if materials {
                let selected = self.selected.ok_or("Select a floor or wall")?;
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .set_material(selected, &id)?;
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

    fn page_library(&mut self, forward: bool) {
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
                    page.saturating_add(1).min(count.saturating_sub(1) / 10)
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
        if let Some(Popup::Library { page, materials: _ }) = &mut self.popup {
            *page = if forward {
                page.saturating_add(1).min(count.saturating_sub(1) / 10)
            } else {
                page.saturating_sub(1)
            };
        }
    }

    fn start_build(&mut self, play: bool) -> Result<(), String> {
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
                "Offset from room floor (m)"
            }
            4 if matches!(self.selected, Some(super::document::ObjectId::Ramp(_))) => {
                "Signed ramp rise (m)"
            }
            6 => "Yaw / turn (degrees)",
            _ => "Transform value",
        }
    }

    pub(super) fn field(&self, field: usize) -> Result<f32, String> {
        if field >= 100 {
            return self.gameplay_number(field);
        }
        if (7..=10).contains(&field) {
            return self
                .opening_rect
                .as_ref()
                .and_then(|rect| rect.get(field.saturating_sub(7)))
                .copied()
                .ok_or_else(|| "Open floor shape tools first".into());
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
                .ok_or("Open floor shape tools first")? = snapped;
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

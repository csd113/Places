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
    Unsaved(Pending),
    Number { field: usize, value: String },
    Library { materials: bool, page: usize },
}

impl Popup {
    pub fn draw(&self, app: &App, v: &mut Vec<crate::engine::render::Vertex>, hits: &mut Vec<Hit>) {
        hits.clear();
        ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.6]);
        ui::rect(v, [130.0, 65.0, 260.0, 146.0], [0.075, 0.11, 0.14, 1.0]);
        match self {
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
            Self::Number { value, field: _ } => {
                ui::text(
                    v,
                    "Numeric value / Enter apply / Esc cancel",
                    140.0,
                    80.0,
                    ui::INK,
                    65,
                );
                ui::text(v, value, 140.0, 104.0, [0.4, 0.9, 0.72, 1.0], 60);
                draw_numeric_hint(v);
            }
            Self::Library { materials, page } => {
                ui::text(
                    v,
                    if *materials {
                        "BUILT-IN MATERIALS / apply to selection"
                    } else {
                        "BUILT-IN PROPS / select then + Prop"
                    },
                    140.0,
                    73.0,
                    ui::INK,
                    65,
                );
                for (index, label) in app
                    .library(*materials)
                    .into_iter()
                    .enumerate()
                    .skip(page.saturating_mul(10))
                    .take(10)
                {
                    let offset = index % 10;
                    let y = f32::from(u16::try_from(offset).unwrap_or(0)).mul_add(10.0, 85.0);
                    ui::popup_button(
                        v,
                        hits,
                        [140.0, y, 240.0, 9.0],
                        &label,
                        Action::ChooseAsset(index),
                    );
                }
                ui::popup_button(
                    v,
                    hits,
                    [140.0, 190.0, 50.0, 12.0],
                    "< Prev",
                    Action::Page(false),
                );
                ui::popup_button(
                    v,
                    hits,
                    [196.0, 190.0, 50.0, 12.0],
                    "Next >",
                    Action::Page(true),
                );
                ui::popup_button(
                    v,
                    hits,
                    [330.0, 190.0, 50.0, 12.0],
                    "Close",
                    Action::ClosePopup,
                );
            }
        }
    }
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
        self.status = format!(
            "Created {}. Add walls and props; click values to edit.",
            path.display()
        );
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
                        let _sent = sender.send((new, result));
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
                    entry.asset_type.as_str() == AssetType::PROP
                }
            })
            .map(|entry| entry.id.clone())
            .collect()
    }

    pub(super) fn activate(&mut self, action: Action, window: &Window) -> Result<(), String> {
        match action {
            Action::New => self.request(Pending::New, window),
            Action::Open => self.request(Pending::Open, window),
            Action::Save => self.save()?,
            Action::Capture => self.request_capture(),
            Action::Undo | Action::Redo => {
                if let Some(document) = &mut self.document {
                    let _changed = if action == Action::Undo {
                        document.undo()
                    } else {
                        document.redo()
                    };
                    self.selected = None;
                    self.changed();
                }
            }
            Action::Add(kind) => self.add_object(kind)?,
            Action::Select(id) => {
                self.selected = Some(id);
                self.viewport.select(Some(id));
            }
            Action::Delete | Action::Duplicate => {
                let id = self.selected.ok_or("Select an object first")?;
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                if action == Action::Delete {
                    doc.delete(id)?;
                    self.selected = None;
                } else {
                    self.selected = Some(doc.duplicate(id)?);
                }
                self.changed();
            }
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
            Action::ClosePopup | Action::ConfirmCancel => self.popup = None,
            Action::ConfirmSave | Action::ConfirmDiscard => {
                if let Some(Popup::Unsaved(pending)) = self.popup {
                    if action == Action::ConfirmSave {
                        self.save()?;
                    }
                    self.popup = None;
                    self.perform(pending, window);
                }
            }
            Action::Level => self.next_level()?,
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
        }
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
        let amount = if field == 6 {
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
        let mut target = self.viewport.target();
        target[1] = if kind == ObjectKind::Prop {
            0.0
        } else {
            self.document
                .as_ref()
                .and_then(|doc| {
                    crate::engine::level::LevelSurfaces::new(doc.level())
                        .floor_y_at(target[0], target[2])
                })
                .unwrap_or(0.0)
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
        self.status = format!("Added {kind:?}. Click numeric fields or use +/- to transform.");
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
                self.status = format!("Selected {id}; click + Prop to place it.");
                self.prop = id;
            }
        }
        self.popup = None;

        Ok(())
    }

    fn page_library(&mut self, forward: bool) {
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

    fn next_level(&mut self) -> Result<(), String> {
        if self.compile.is_some() || self.play.is_some() {
            return Err("Finish the build or play session before switching levels".into());
        }
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        let ids: Vec<_> = doc.level_ids().collect();
        let index = ids.iter().position(|id| *id == doc.level_id()).unwrap_or(0);
        let next = ids
            .get(
                index
                    .saturating_add(1)
                    .checked_rem(ids.len().max(1))
                    .unwrap_or(0),
            )
            .ok_or("No project levels")?
            .to_string();
        doc.select_level(&next)?;
        self.selected = None;
        self.changed();
        self.viewport.frame_all();

        Ok(())
    }

    fn start_build(&mut self, play: bool) -> Result<(), String> {
        if self.compile.is_some() || self.play.is_some() {
            return Err("A build or player is already running".into());
        }
        self.save()?;
        let doc = self.document.as_ref().ok_or("Open a project first")?;
        self.compile = Some(CompileJob::start(
            &self.tools,
            doc.project(),
            doc.level_id(),
        )?);
        self.play_after_build = play;
        self.status = "Compiling saved source...".into();

        Ok(())
    }

    pub(super) fn field(&self, field: usize) -> Result<f32, String> {
        let transform = self
            .document
            .as_ref()
            .ok_or("Open a project first")?
            .transform(self.selected.ok_or("Select an object")?)?;
        match field {
            0..=2 => transform.position.get(field).copied(),
            3..=5 => transform.size.get(field.saturating_sub(3)).copied(),
            6 => Some(transform.yaw_degrees),
            _ => None,
        }
        .ok_or_else(|| "Unknown transform field".into())
    }

    pub(super) fn apply_field(&mut self, field: usize, mut value: f32) -> Result<(), String> {
        if !value.is_finite() {
            return Err("Enter a finite number".into());
        }
        if self.view.snap && field < 6 {
            value = (value / self.step).round() * self.step;
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
                    "{} {}{}",
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
                    BuildOutcome::Failed(error) => {
                        crate::engine::logging::warn(&error);
                        self.status = format!("Build failed: {error}");
                    }
                    BuildOutcome::Cancelled => {
                        self.status =
                            "Build cancelled. Source and last published package retained.".into();
                    }
                }
                self.play_after_build = false;
            }
        }
        if let Some(job) = &mut self.play
            && let Some(outcome) = job.poll()
        {
            self.status = match outcome {
                PlayOutcome::Returned => "Returned to editing.".into(),
                PlayOutcome::Stopped => "Play stopped. Editor work retained.".into(),
                PlayOutcome::Failed(error) => format!("Play failed: {error}"),
            };
            self.play = None;
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

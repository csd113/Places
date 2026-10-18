//! Project controls grouped away from the scene and transform workspace.
use std::path::{Path, PathBuf};

use sdl3::event::Event;
use sdl3::keyboard::Keycode;

use super::{
    App,
    actions::Popup,
    process::CompileJob,
    ui::{self, Action, Hit},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Field {
    LevelTitle,
    Name,
    Author,
    Description,
    Thumbnail,
    AddLevel,
    Export,
    Import,
    ImportJson,
    Destination,
    Transitions,
}
pub(super) enum Operation {
    Export,
    Import,
    Check,
}

impl Field {
    fn metadata_key(self) -> Result<&'static str, String> {
        match self {
            Self::Name => Ok("name"),
            Self::LevelTitle => Ok("display_name"),
            Self::Author => Ok("author"),
            Self::Description => Ok("description"),
            Self::Thumbnail
            | Self::AddLevel
            | Self::Export
            | Self::Import
            | Self::ImportJson
            | Self::Destination
            | Self::Transitions => Err("This project operation is not a metadata field".into()),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Name => "Project name",
            Self::LevelTitle => "Selected map menu title (blank uses stable ID)",
            Self::Author => "Author",
            Self::Description => "Description",
            Self::Thumbnail => "Selected level thumbnail: existing PNG file path",
            Self::AddLevel => "New level stable ID (lowercase letters, digits, _ or -)",
            Self::Export => "Export complete project to .placesmap",
            Self::Import => "Recover editable sources from project .placesmap",
            Self::ImportJson => "Import standalone JSON map (keeps its stable ID and source bytes)",
            Self::Transitions => "Allowed destinations for this level (comma-separated stable IDs)",
            Self::Destination => "Recover into NEW folder (existing folders are refused)",
        }
    }
}

pub(super) fn draw(app: &App, v: &mut Vec<crate::engine::render::Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.7]);
    ui::rect(v, [20.0, 26.0, 440.0, 210.0], [0.075, 0.11, 0.14, 1.0]);
    if matches!(app.popup, Some(Popup::Thumbnail)) {
        draw_thumbnail(app, v, hits);
        return;
    }
    ui::text(
        v,
        "PROJECT / SOURCES AND DISTRIBUTION",
        30.0,
        36.0,
        ui::INK,
        100,
    );
    if let Some(Popup::ProjectText { field, value }) = &app.popup {
        draw_text(v, hits, *field, value);
    } else {
        draw_project(app, v, hits);
    }
    ui::popup_button(
        v,
        hits,
        [388.0, 219.0, 60.0, 12.0],
        "Close",
        Action::ClosePopup,
    );
}

fn draw_thumbnail(
    app: &App,
    vertices: &mut Vec<crate::engine::render::Vertex>,
    hits: &mut Vec<Hit>,
) {
    ui::text(
        vertices,
        "SELECTED LEVEL / MENU IMAGE",
        30.0,
        36.0,
        ui::INK,
        100,
    );
    for (left, width, label, action) in [
        (30.0, 100.0, "Choose PNG", Action::ThumbnailChoose),
        (
            136.0,
            100.0,
            "Paste path",
            Action::ProjectField(Field::Thumbnail),
        ),
        (242.0, 100.0, "Remove", Action::ThumbnailRemove),
        (348.0, 100.0, "Back", Action::Project),
    ] {
        ui::popup_button(vertices, hits, [left, 52.0, width, 14.0], label, action);
    }
    ui::text(
        vertices,
        "16:9 PNG, up to 1280x720 / embedded on export",
        30.0,
        73.0,
        ui::INK,
        116,
    );
    ui::rect(
        vertices,
        [112.0, 86.0, 256.0, 144.0],
        [0.04, 0.065, 0.08, 1.0],
    );
    if app.thumbnail_image.is_none() {
        ui::text(
            vertices,
            "No valid image / menu fallback",
            132.0,
            148.0,
            ui::INK,
            60,
        );
    }
}

fn draw_text(
    v: &mut Vec<crate::engine::render::Vertex>,
    hits: &mut Vec<Hit>,
    field: Field,
    value: &str,
) {
    ui::text(v, field.label(), 30.0, 60.0, ui::INK, 116);
    ui::rect(v, [30.0, 78.0, 420.0, 36.0], [0.04, 0.065, 0.08, 1.0]);
    for (row, line) in value
        .chars()
        .collect::<Vec<_>>()
        .chunks(101)
        .rev()
        .take(5)
        .rev()
        .enumerate()
    {
        ui::text(
            v,
            &line.iter().collect::<String>(),
            34.0,
            f32::from(u16::try_from(row).unwrap_or(0)).mul_add(7.0, 80.0),
            ui::INK,
            116,
        );
    }
    ui::text(
        v,
        "Type or paste. Cmd/Ctrl A clears; Enter applies; Esc returns.",
        30.0,
        124.0,
        ui::INK,
        116,
    );
    ui::text(
        v,
        "Metadata operations save current edits first. Built-in runtime assets only.",
        30.0,
        137.0,
        ui::INK,
        116,
    );
    ui::popup_button(
        v,
        hits,
        [30.0, 160.0, 100.0, 14.0],
        "Apply",
        Action::ProjectApply,
    );
    ui::popup_button(
        v,
        hits,
        [140.0, 160.0, 100.0, 14.0],
        "Back",
        Action::Project,
    );
}
fn draw_project(app: &App, v: &mut Vec<crate::engine::render::Vertex>, hits: &mut Vec<Hit>) {
    if let Some(doc) = &app.document {
        ui::text(
            v,
            &format!(
                "{} | {} level(s) | entry: {}",
                doc.manifest().id,
                doc.manifest().levels.len(),
                doc.manifest().entry_level
            ),
            30.0,
            49.0,
            ui::INK,
            116,
        );
        for (row, (label, field)) in [
            ("Name", Field::Name),
            ("Author", Field::Author),
            ("Description", Field::Description),
            ("Level menu image >", Field::Thumbnail),
            ("Add level", Field::AddLevel),
            ("Selected map title", Field::LevelTitle),
        ]
        .into_iter()
        .enumerate()
        {
            ui::popup_button(
                v,
                hits,
                [
                    30.0,
                    f32::from(u16::try_from(row).unwrap_or(0)).mul_add(15.0, 67.0),
                    190.0,
                    13.0,
                ],
                label,
                if field == Field::Thumbnail {
                    Action::Thumbnail
                } else {
                    Action::ProjectField(field)
                },
            );
        }
        draw_level(doc, v, hits);
    } else {
        ui::popup_button(
            v,
            hits,
            [238.0, 67.0, 210.0, 14.0],
            "Recover editable project",
            Action::ProjectField(Field::Import),
        );
    }
    ui::text(
        v,
        "Source recovery preserves authored bytes. Play loads compiled levels on demand.",
        30.0,
        211.0,
        ui::INK,
        116,
    );
}

fn draw_level(
    doc: &super::document::Document,
    v: &mut Vec<crate::engine::render::Vertex>,
    hits: &mut Vec<Hit>,
) {
    ui::popup_button(
        v,
        hits,
        [30.0, 195.0, 190.0, 13.0],
        "Map settings >",
        Action::Components(10),
    );
    ui::popup_button(
        v,
        hits,
        [30.0, 159.0, 190.0, 13.0],
        &format!("Selected: {} / Next >", doc.level_id()),
        Action::ProjectSelect(
            doc.level_ids()
                .position(|id| id == doc.level_id())
                .unwrap_or(0)
                .saturating_add(1)
                .checked_rem(doc.manifest().levels.len().max(1))
                .unwrap_or(0),
        ),
    );
    ui::popup_button(
        v,
        hits,
        [30.0, 177.0, 190.0, 13.0],
        "Use selected as entry",
        Action::ProjectEntry,
    );
    ui::popup_button(
        v,
        hits,
        [238.0, 177.0, 210.0, 13.0],
        if doc.level().crosshair {
            "Map crosshair: On"
        } else {
            "Map crosshair: Off"
        },
        Action::MapCrosshair,
    );
    for (row, (label, action)) in [
        (
            "Export all levels + sources",
            Action::ProjectField(Field::Export),
        ),
        (
            "Recover editable project",
            Action::ProjectField(Field::Import),
        ),
        ("Check project", Action::ProjectCheck),
        ("Migration check", Action::ProjectMigrate),
        ("Import JSON map", Action::ProjectField(Field::ImportJson)),
    ]
    .into_iter()
    .enumerate()
    {
        ui::popup_button(
            v,
            hits,
            [
                238.0,
                f32::from(u16::try_from(row).unwrap_or(0)).mul_add(18.0, 67.0),
                210.0,
                14.0,
            ],
            label,
            action,
        );
    }
    ui::popup_button(
        v,
        hits,
        [238.0, 159.0, 210.0, 13.0],
        "Allowed transitions >",
        Action::ProjectField(Field::Transitions),
    );
    ui::popup_button(
        v,
        hits,
        [238.0, 195.0, 210.0, 13.0],
        if doc.manifest().menu_entry_only {
            "Explore menu: Entry only"
        } else {
            "Explore menu: All maps"
        },
        Action::ProjectMenuEntryOnly,
    );
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only project actions are handled here before the central dispatcher."
    )]
    pub(super) fn project_action(
        &mut self,
        action: Action,
        window: &sdl3::video::Window,
    ) -> Result<bool, String> {
        match action {
            Action::Thumbnail => {
                self.refresh_thumbnail_preview();
                self.popup = Some(Popup::Thumbnail);
            }
            Action::ThumbnailChoose => self.choose_thumbnail(window)?,
            Action::ThumbnailRemove => {
                self.document
                    .as_mut()
                    .ok_or("Open a project first")?
                    .remove_thumbnail()?;
                self.refresh_thumbnail_preview();
                self.status = "Level image removed; the menu uses its fallback.".into();
            }
            Action::MapCrosshair => {
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                doc.set_crosshair(!doc.level().crosshair)?;
            }
            Action::Project => {
                if self.compile.is_some() || self.play.is_some() {
                    return Err("Finish the build or play session before project operations".into());
                }
                self.popup = Some(Popup::Project);
            }
            Action::ProjectField(field) => {
                let value = self
                    .document
                    .as_ref()
                    .map_or_else(String::new, |doc| match field {
                        Field::Name => doc.manifest().name.clone(),
                        Field::LevelTitle => doc
                            .manifest()
                            .level(Some(doc.level_id()))
                            .map(|record| record.display_name.clone())
                            .unwrap_or_default(),
                        Field::Author => doc.manifest().author.clone(),
                        Field::Description => doc.manifest().description.clone(),
                        Field::Export => doc
                            .root()
                            .join(format!("{}.placesmap", doc.manifest().id))
                            .display()
                            .to_string(),
                        Field::Transitions => doc.level().transitions.join(", "),
                        Field::Thumbnail
                        | Field::AddLevel
                        | Field::Import
                        | Field::ImportJson
                        | Field::Destination => String::new(),
                    });
                self.popup = Some(Popup::ProjectText { field, value });
            }
            Action::ProjectApply => self.apply_project_text()?,
            Action::ProjectEntry => {
                self.save()?;
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                let entry_id = doc.level_id().to_string();
                doc.update_manifest("entry_level", &entry_id)?;
                self.status = "Entry level saved.".into();
            }
            Action::ProjectMenuEntryOnly => {
                self.save()?;
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                doc.set_menu_entry_only(!doc.manifest().menu_entry_only)?;
                self.status = if doc.manifest().menu_entry_only {
                    "Explore opens the entry map; internal destinations remain available.".into()
                } else {
                    "Explore lists every project map.".into()
                };
            }
            Action::ProjectSelect(index) => {
                let doc = self.document.as_mut().ok_or("Open a project first")?;
                let id = doc
                    .level_ids()
                    .nth(index)
                    .ok_or("Unknown level")?
                    .to_string();
                doc.select_level(&id)?;
                self.object_query.clear();
                self.selected = None;
                self.object_scroll = 0;
                self.changed();
                self.viewport.frame_all();
            }
            Action::ProjectCheck | Action::ProjectMigrate => {
                self.save()?;
                let doc = self.document.as_ref().ok_or("Open a project first")?;
                self.compile = Some(CompileJob::operation(
                    &self.tools,
                    "check-project",
                    doc.root(),
                    doc.root(),
                )?);
                self.status = "Checking complete project...".into();
                self.project_operation = Some(Operation::Check);
                self.popup = None;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn apply_project_text(&mut self) -> Result<(), String> {
        let Some(Popup::ProjectText {
            field: source_field,
            value: source_value,
        }) = &self.popup
        else {
            return Ok(());
        };
        let field = *source_field;
        let value = source_value.clone();
        if field == Field::Import {
            let source = PathBuf::from(value);
            let index = crate::engine::package::project::inspect(&source)?
                .ok_or("Ordinary compiled maps have no editable project sources")?;
            let destination = source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}-recovered", index.project.id));
            self.project_import = Some(source);
            self.popup = Some(Popup::ProjectText {
                field: Field::Destination,
                value: destination.display().to_string(),
            });
            return Ok(());
        }
        if field == Field::Destination {
            if self
                .document
                .as_ref()
                .is_some_and(super::document::Document::is_dirty)
            {
                self.save()?;
            }
            self.compile = Some(CompileJob::operation(
                &self.tools,
                "import-project",
                self.project_import
                    .as_deref()
                    .ok_or("Choose a project package first")?,
                Path::new(&value),
            )?);
            self.status = "Validating and recovering editable project...".into();
            self.project_operation = Some(Operation::Import);
            self.popup = None;
            return Ok(());
        }
        self.save()?;
        let doc = self.document.as_mut().ok_or("Open a project first")?;
        match field {
            Field::Name | Field::Author | Field::Description | Field::LevelTitle => {
                doc.update_manifest(field.metadata_key()?, &value)?;
            }
            Field::Transitions => {
                doc.set_transitions(&value)?;
                doc.save_all()?;
                self.changed();
            }
            Field::Thumbnail => doc.attach_thumbnail(Path::new(&value))?,
            Field::AddLevel => {
                doc.add_level(&value)?;
                self.selected = None;
                self.changed();
            }
            Field::ImportJson => {
                let _imported_id = doc.import_level(Path::new(&value))?;
                self.object_query.clear();
                self.selected = None;
                self.object_scroll = 0;
                self.changed();
                self.viewport.frame_all();
            }
            Field::Export => {
                self.compile = Some(CompileJob::operation(
                    &self.tools,
                    "export-project",
                    doc.root(),
                    Path::new(&value),
                )?);
                self.status = "Exporting all levels and editable sources...".into();
                self.project_operation = Some(Operation::Export);
                self.popup = None;
                return Ok(());
            }
            Field::Import | Field::Destination => {
                return Err("Invalid project operation state".into());
            }
        }
        self.status = "Project changes saved.".into();
        if field == Field::Thumbnail {
            self.refresh_thumbnail_preview();
            self.popup = Some(Popup::Thumbnail);
        } else {
            self.popup = Some(Popup::Project);
        }
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only text entry events are consumed by this modal."
    )]
    pub(super) fn project_text_event(&mut self, event: &Event) -> bool {
        let Some(Popup::ProjectText { field: _, value }) = &mut self.popup else {
            return false;
        };
        let mut apply = false;
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } if value.len().saturating_add(text.len()) <= 4096 => {
                value.push_str(text);
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                let _removed = value.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if super::input::command(*keymod) => value.clear(),
            Event::KeyDown {
                keycode: Some(Keycode::V),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if super::input::command(*keymod) => {
                if let Ok(context) = sdl3::init()
                    && let Ok(video) = context.video()
                    && let Ok(text) = video.clipboard().clipboard_text()
                    && value.len().saturating_add(text.len()) <= 4096
                {
                    value.push_str(&text);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } => apply = true,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => self.popup = Some(Popup::Project),
            _ => {}
        }
        if apply {
            let result = self.apply_project_text();
            self.report(result);
        }
        // Mouse events must still reach visible Apply/Back/Close buttons.
        is_text_event(event)
    }
}

const fn is_text_event(event: &Event) -> bool {
    matches!(
        event,
        Event::TextInput {
            timestamp: _,
            window_id: _,
            text: _
        } | Event::KeyDown {
            timestamp: _,
            window_id: _,
            keycode: _,
            scancode: _,
            keymod: _,
            repeat: _,
            which: _,
            raw: _
        } | Event::KeyUp {
            timestamp: _,
            window_id: _,
            keycode: _,
            scancode: _,
            keymod: _,
            repeat: _,
            which: _,
            raw: _
        }
    )
}

impl App {
    pub(super) fn refresh_thumbnail_preview(&mut self) {
        let result = self
            .document
            .as_ref()
            .ok_or_else(|| "Open a project first".to_string())
            .and_then(|doc| {
                let record = doc.manifest().level(Some(doc.level_id()))?;
                let path = record.thumbnail.as_ref().or_else(|| {
                    (doc.level_id() == doc.manifest().entry_level)
                        .then_some(doc.manifest().thumbnail.as_ref())
                        .flatten()
                });
                path.map(|source| {
                    let bytes = crate::engine::project::read_bounded_file(
                        &doc.project().resolve(source)?,
                        crate::engine::package::project::MAX_THUMBNAIL_BYTES,
                    )?;
                    crate::engine::package::project::decode_thumbnail(&bytes)
                        .map(std::sync::Arc::new)
                })
                .transpose()
            });
        match result {
            Ok(image) => self.thumbnail_image = image,
            Err(error) => {
                self.thumbnail_image = None;
                self.status = format!("Image preview: {error}");
            }
        }
    }

    fn choose_thumbnail(&mut self, window: &sdl3::video::Window) -> Result<(), String> {
        if self.dialog_open {
            return Ok(());
        }
        let sender = self.dialog_sender.clone();
        sdl3::dialog::show_open_file_dialog(
            &[sdl3::dialog::DialogFileFilter {
                name: "Level PNG image (16:9)",
                pattern: "png",
            }],
            None::<&Path>,
            false,
            window,
            Box::new(move |result, _filter| {
                let _sent = sender.send((super::DialogPurpose::Thumbnail, result));
            }),
        )
        .map_err(|error| error.to_string())?;
        self.dialog_open = true;
        Ok(())
    }
}

//! Docked authoring chrome; presentation state never enters document history.
use super::{ACCENT, Action, Hit, INK, MUTED, button, rect, text};
use crate::editor::{App, construction::Tool, document::ObjectKind};
use crate::engine::render::Vertex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::editor) enum Tab {
    #[default]
    Build,
    Assets,
    Lighting,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::editor) enum Catalog {
    #[default]
    Materials,
    Models,
    Fixtures,
    Doors,
}

#[derive(Default)]
pub(in crate::editor) struct State {
    pub tab: Tab,
    pub catalog: Catalog,
    pub page: usize,
    pub search: bool,
    pub advanced: bool,
    pub numeric: bool,
}

pub(super) fn shell(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    toolbar(app, v, hits);
    if app.navigation.visible {
        super::navigation::draw(app, v, hits);
    } else {
        left(app, v, hits);
    }
    let first_vertex = v.len();
    let first_hit = hits.len();
    super::inspector::inspector(app, v, hits);
    anchor_right(
        app,
        v.get_mut(first_vertex..).unwrap_or_default(),
        hits.get_mut(first_hit..).unwrap_or_default(),
    );
    if app.compile.is_some() || app.viewport.local_progress().is_some() {
        return;
    }
    let footer_vertex = v.len();
    let footer_hit = hits.len();
    footer(app, v, hits);
    anchor_bottom(
        app,
        v.get_mut(footer_vertex..).unwrap_or_default(),
        hits.get_mut(footer_hit..).unwrap_or_default(),
    );
}

fn toolbar(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(v, "PLACES / EDITOR", 5.0, 4.0, ACCENT, 28);
    let title = app.document.as_ref().map_or_else(
        || "Create or open a project".into(),
        |d| format!("{} / {}", d.project().manifest().name, d.level_id()),
    );
    text(v, &title, 122.0, 4.0, INK, 65);
    let saved_start = v.len();
    text(
        v,
        if app
            .document
            .as_ref()
            .is_some_and(crate::editor::Document::is_dirty)
        {
            "Unsaved changes"
        } else {
            "Saved"
        },
        393.0,
        4.0,
        MUTED,
        21,
    );
    anchor_right(app, v.get_mut(saved_start..).unwrap_or_default(), &mut []);
    let open = app.document.is_some();
    let idle = app.compile.is_none() && app.play.is_none() && !app.dialog_open;
    for (x, width, label, action, enabled) in [
        (5.0, 24.0, "New", Action::New, idle),
        (32.0, 26.0, "Open", Action::Open, idle),
        (61.0, 42.0, "Project >", Action::Project, idle),
        (
            115.0,
            28.0,
            "Undo",
            Action::Undo,
            app.document
                .as_ref()
                .is_some_and(crate::editor::Document::can_undo),
        ),
        (
            146.0,
            28.0,
            "Redo",
            Action::Redo,
            app.document
                .as_ref()
                .is_some_and(crate::editor::Document::can_redo),
        ),
        (
            177.0,
            28.0,
            "Save",
            Action::Save,
            open && app.compile.is_none(),
        ),
        (
            209.0,
            63.0,
            "Tools >",
            Action::Workspace,
            open && app.compile.is_none() && app.play.is_none(),
        ),
        (275.0, 83.0, "Test in editor", Action::Play, idle && open),
        (362.0, 43.0, "Here >", Action::PlayOptions, idle && open),
        (410.0, 65.0, "Final build", Action::Build, idle && open),
    ] {
        let first_vertex = v.len();
        let first_hit = hits.len();
        button(v, hits, [x, 16.0, width, 12.0], label, action, enabled);
        if x >= 275.0 {
            anchor_right(
                app,
                v.get_mut(first_vertex..).unwrap_or_default(),
                hits.get_mut(first_hit..).unwrap_or_default(),
            );
        }
    }
    if app.compile.is_some() || app.play.is_some() {
        button(
            v,
            hits,
            [209.0, 16.0, 63.0, 12.0],
            if app.play.is_some() {
                "Stop test"
            } else {
                "Cancel build"
            },
            if app.play.is_some() {
                Action::Stop
            } else {
                Action::Cancel
            },
            true,
        );
    }
}

fn left(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (x, width, label, tab) in [
        (4.0, 27.0, "Build", Tab::Build),
        (33.0, 30.0, "Assets", Tab::Assets),
        (65.0, 39.0, "Lighting", Tab::Lighting),
    ] {
        let tab_label = if app.dock.tab == tab {
            label.to_ascii_uppercase()
        } else {
            label.to_owned()
        };
        button(
            v,
            hits,
            [x, 36.0, width, 13.0],
            &tab_label,
            Action::DockTab(tab),
            true,
        );
        if app.dock.tab == tab {
            rect(v, [x, 48.0, width, 0.8], ACCENT);
        }
    }
    let open = app.document.is_some();
    primary_tools(app, v, hits);
    super::library::draw_dock(app, v, hits);
    if app.dock.tab == Tab::Assets && app.dock.catalog == Catalog::Models {
        button(
            v,
            hits,
            [5.0, 185.0, 94.0, 13.0],
            "Place chosen prop",
            Action::Add(ObjectKind::Prop),
            open,
        );
    }
    if app.compile.is_some() || app.viewport.local_progress().is_some() {
        return;
    }
    button(
        v,
        hits,
        [5.0, 237.0, 45.0, 11.0],
        "Scene >",
        Action::SceneSearch,
        open,
    );
    button(
        v,
        hits,
        [54.0, 237.0, 45.0, 11.0],
        "Layers >",
        Action::Layers,
        open,
    );
}

fn primary_tools(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let open = app.document.is_some();
    match app.dock.tab {
        Tab::Build => {
            text(v, "GEOMETRY", 5.0, 51.0, MUTED, 23);
            for (y, label, action) in [
                (59.0, "Room", Action::Add(ObjectKind::Room)),
                (73.0, "Wall", Action::Place(Tool::Wall)),
                (87.0, "Door", Action::Place(Tool::Door)),
                (101.0, "Stairs >", Action::StairWizard),
                (115.0, "Doorway opening", Action::Place(Tool::Doorway)),
            ] {
                button(
                    v,
                    hits,
                    [
                        5.0,
                        y,
                        if action == Action::Add(ObjectKind::Room) {
                            45.0
                        } else {
                            94.0
                        },
                        11.0,
                    ],
                    label,
                    action,
                    open,
                );
            }
            button(
                v,
                hits,
                [54.0, 59.0, 45.0, 11.0],
                "Elevator",
                Action::Elevator(crate::editor::elevators::Command::Place),
                open,
            );
        }
        Tab::Assets => {
            for (x, width, label, catalog) in [
                (5.0, 29.0, "Props", Catalog::Models),
                (36.0, 34.0, "Surface", Catalog::Materials),
                (72.0, 27.0, "Doors", Catalog::Doors),
            ] {
                button(
                    v,
                    hits,
                    [x, 55.0, width, 12.0],
                    label,
                    Action::DockCatalog(catalog),
                    open,
                );
                if app.dock.catalog == catalog {
                    rect(v, [x, 66.0, width, 0.6], ACCENT);
                }
            }
        }
        Tab::Lighting => {
            button(
                v,
                hits,
                [5.0, 56.0, 94.0, 13.0],
                "Place light fixture",
                Action::Place(Tool::Light),
                open,
            );
            button(
                v,
                hits,
                [5.0, 73.0, 94.0, 12.0],
                "Circuits / preview >",
                Action::Lighting(crate::editor::lighting::Command::Open),
                open,
            );
            text(v, "Select a light to assign", 5.0, 89.0, MUTED, 24);
        }
    }
}

fn footer(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let selected = app.selected.and_then(|id| {
        app.document
            .as_ref()?
            .objects()
            .iter()
            .find(|object| object.id == id)
    });
    text(
        v,
        selected.map_or("Click an object to inspect", |object| object.label.as_str()),
        5.0,
        255.0,
        MUTED,
        27,
    );
    for (x, width, label, action) in [
        (
            119.0,
            46.0,
            if app.view.grid {
                "Grid: on".into()
            } else {
                "Grid: off".into()
            },
            Action::Grid,
        ),
        (
            169.0,
            46.0,
            if app.view.snap {
                "Snap: on".into()
            } else {
                "Snap: off".into()
            },
            Action::Snap,
        ),
        (219.0, 53.0, format!("Step {:.2}m", app.step), Action::Step),
        (
            276.0,
            98.0,
            if app.viewport.environment_preview() {
                "View: authored".into()
            } else {
                "View: inspect".into()
            },
            Action::EnvironmentPreview,
        ),
        (421.0, 54.0, "Help >".into(), Action::Help),
    ] {
        let first_vertex = v.len();
        let first_hit = hits.len();
        button(v, hits, [x, 253.0, width, 10.0], &label, action, true);
        if x >= 421.0 {
            anchor_right(
                app,
                v.get_mut(first_vertex..).unwrap_or_default(),
                hits.get_mut(first_hit..).unwrap_or_default(),
            );
        }
    }
    text(v, &app.status, 5.0, 265.0, INK, 132);
}

pub(in crate::editor) fn help(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    rect(v, [118.0, 49.0, 268.0, 182.0], super::PANEL);
    text(v, "EDITOR HELP", 128.0, 59.0, ACCENT, 60);
    for (row, line) in [
        "Build: Room, Elevator, Wall, Door, Stairs and openings.",
        "Assets: choose a prop, then Place chosen prop.",
        "A selected prop receives the chosen model immediately.",
        "Select an object for dimensions, surfaces and settings.",
        "Advanced / All tools keeps deeper authoring available.",
        "Right drag orbit / middle drag pan / wheel zoom / F frame",
        "Ctrl/Cmd S save / Z undo / Shift Z redo / D duplicate",
        "Arrows move / Page Up-Down height / R rotate / +/- scale",
        "Tab / Shift Tab focus / Enter activate / Esc cancel",
        "Test in editor: unbaked gameplay. Here selects spawn.",
        "Final build: production preparation; Cancel stays visible.",
    ]
    .into_iter()
    .enumerate()
    {
        text(
            v,
            line,
            128.0,
            f32::from(u16::try_from(row).unwrap_or(0)).mul_add(12.0, 78.0),
            INK,
            63,
        );
    }
    button(
        v,
        hits,
        [320.0, 216.0, 55.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

fn anchor_right(app: &App, vertices: &mut [Vertex], hits: &mut [Hit]) {
    let offset = app.viewport.layout().right_offset();
    for vertex in vertices {
        vertex.pos[0] += offset;
    }
    for hit in hits {
        hit.rect[0] += offset;
    }
}

pub(in crate::editor) fn anchor_bottom(app: &App, vertices: &mut [Vertex], hits: &mut [Hit]) {
    let offset = app.viewport.layout().bottom_offset();
    for vertex in vertices {
        vertex.pos[1] += offset;
    }
    for hit in hits {
        hit.rect[1] += offset;
    }
}

use crate::editor::ui::{self, Action, Hit};
use crate::editor::{App, actions::Popup};
use crate::engine::render::Vertex;

pub(in crate::editor) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [0.0, 38.0, 480.0, 204.0], [0.055, 0.08, 0.11, 1.0]);
    let Some(doc) = &app.document else {
        return;
    };
    match &app.popup {
        Some(Popup::Scripts) => {
            draw_modules(doc, v, hits);
        }
        Some(Popup::ScriptHandlers {
            path: _,
            names,
            page,
        }) => {
            ui::text(
                v,
                "LINK TO SELECTED EVENT / handler exports",
                8.0,
                43.0,
                ui::INK,
                125,
            );
            ui::popup_button(
                v,
                hits,
                [8.0, 53.0, 45.0, 11.0],
                "Previous",
                Action::Page(false),
            );
            ui::popup_button(
                v,
                hits,
                [57.0, 53.0, 45.0, 11.0],
                "Next",
                Action::Page(true),
            );
            {
                for (index, name) in names
                    .iter()
                    .enumerate()
                    .skip(page.saturating_mul(20))
                    .take(20)
                {
                    ui::popup_button(
                        v,
                        hits,
                        [
                            8.0,
                            f32::from(u16::try_from(index % 20).unwrap_or(0)).mul_add(7.3, 80.0),
                            460.0,
                            7.0,
                        ],
                        name,
                        Action::ScriptHandler(index),
                    );
                }
            }
        }
        Some(Popup::ScriptEdit {
            path,
            cursor,
            scroll,
            all,
        }) => {
            draw_source(doc, v, hits, path, *cursor, *scroll, *all);
        }
        Some(Popup::ScriptDiagnostics) => {
            draw_diagnostics(app, v, hits);
        }
        _ => {}
    }
    ui::popup_button(
        v,
        hits,
        [420.0, 53.0, 48.0, 11.0],
        "Close",
        Action::ClosePopup,
    );
}

fn draw_source(
    doc: &crate::editor::Document,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    path: &str,
    cursor: usize,
    scroll: usize,
    all: bool,
) {
    ui::text(v, path, 8.0, 43.0, ui::INK, 125);
    for (x, w, label, action) in [
        (8.0, 45.0, "Save", Action::Save),
        (57.0, 45.0, "Check", Action::ScriptCheck),
        (106.0, 70.0, "Link handler >", Action::ScriptLink),
        (180.0, 60.0, "Modules >", Action::Scripts),
        (244.0, 65.0, "Diagnostics", Action::ScriptDiagnostics),
    ] {
        ui::popup_button(v, hits, [x, 53.0, w, 11.0], label, action);
    }
    ui::text(
        v,
        "Enter/Tab edit | arrows/Home/End move | Cmd A/C/V | Cmd S save | Cmd B check | Esc keeps buffer",
        8.0,
        68.0,
        ui::INK,
        130,
    );
    let source = doc.script_text(path).unwrap_or_default();
    let prefix = source.get(..cursor).unwrap_or_default();
    let line = prefix.bytes().filter(|c| *c == b'\n').count();
    let column = prefix
        .rsplit('\n')
        .next()
        .unwrap_or_default()
        .chars()
        .count();
    let horizontal = column.saturating_sub(110);
    for (index, value) in source.split('\n').skip(scroll).take(20).enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(7.5, 80.0);
        ui::text(
            v,
            &format!("{:>3}", scroll.saturating_add(index).saturating_add(1)),
            5.0,
            y,
            [0.4, 0.6, 0.6, 1.0],
            4,
        );
        let display: String = value.chars().skip(horizontal).take(128).collect();
        ui::text(
            v,
            &display,
            20.0,
            y,
            if all {
                [0.37, 0.86, 0.71, 1.0]
            } else {
                ui::INK
            },
            128,
        );
        if scroll.saturating_add(index) == line {
            ui::rect(
                v,
                [
                    f32::from(u16::try_from(column.saturating_sub(horizontal)).unwrap_or(0))
                        .mul_add(4.0, 20.0),
                    y,
                    0.5,
                    5.5,
                ],
                [0.5, 1.0, 0.8, 1.0],
            );
        }
    }
    ui::text(
        v,
        &format!(
            "Line {} column {} / {} bytes{}",
            line.saturating_add(1),
            column.saturating_add(1),
            source.len(),
            if all { " / all selected" } else { "" }
        ),
        8.0,
        233.0,
        ui::INK,
        100,
    );
}

fn draw_diagnostics(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "LUAU DIAGNOSTICS / last check or Play",
        8.0,
        43.0,
        ui::INK,
        125,
    );
    ui::popup_button(
        v,
        hits,
        [8.0, 53.0, 60.0, 11.0],
        "Modules >",
        Action::Scripts,
    );
    let lines: Vec<String> = app
        .script_feedback
        .lines()
        .flat_map(|line| {
            line.chars()
                .collect::<Vec<_>>()
                .chunks(110)
                .map(|chunk| chunk.iter().collect::<String>())
                .collect::<Vec<_>>()
        })
        .take(20)
        .collect();
    for (index, line) in lines.iter().enumerate() {
        ui::text(
            v,
            line,
            8.0,
            f32::from(u16::try_from(index).unwrap_or(0)).mul_add(7.5, 80.0),
            ui::INK,
            128,
        );
    }
}

fn draw_modules(doc: &crate::editor::Document, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "LEVEL SCRIPTS / shared project + selected map",
        8.0,
        43.0,
        ui::INK,
        125,
    );
    ui::popup_button(
        v,
        hits,
        [8.0, 53.0, 70.0, 11.0],
        "+ New module",
        Action::ScriptNew,
    );
    ui::popup_button(
        v,
        hits,
        [82.0, 53.0, 70.0, 11.0],
        "Check all",
        Action::ScriptCheck,
    );
    ui::text(
        v,
        "Select a prop, door or trigger event, open a module, then Link handler.",
        8.0,
        69.0,
        ui::INK,
        126,
    );
    for (index, path) in doc.script_paths().iter().take(16).enumerate() {
        ui::popup_button(
            v,
            hits,
            [
                8.0,
                f32::from(u16::try_from(index).unwrap_or(0)).mul_add(9.0, 80.0),
                460.0,
                8.0,
            ],
            path,
            Action::ScriptOpen(index),
        );
    }
}

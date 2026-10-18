use super::{Action, App, Command, Draft, Hit, Popup, Tool, Value, Vertex, ui};
pub(in crate::editor) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if let Some(Popup::AuthoringForm(draft)) = &app.popup
        && draft.preview
    {
        ui::rect(v, [122.0, 207.0, 348.0, 31.0], [0.075, 0.11, 0.14, 1.0]);
        ui::text(
            v,
            "VALIDATED PREVIEW / source unchanged",
            130.0,
            211.0,
            ui::INK,
            80,
        );
        for (x, label, action) in [
            (130.0, "Edit fields", Action::Author(Command::Field(0))),
            (242.0, "Apply", Action::Author(Command::Apply)),
            (354.0, "Cancel", Action::ClosePopup),
        ] {
            ui::popup_button(v, hits, [x, 222.0, 105.0, 12.0], label, action);
        }
        if draft.tool == Tool::Water {
            draw_water_slice(app, draft, v, hits);
        }
        return;
    }

    ui::rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.65]);
    ui::rect(v, [116.0, 40.0, 354.0, 197.0], [0.075, 0.11, 0.14, 1.0]);
    let Some(Popup::AuthoringForm(draft)) = &app.popup else {
        draw_hub(v, hits);
        return;
    };
    draw_form(draft, v, hits);
}

fn draw_form(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        &format!(
            "{} / record {}",
            draft.tool.title(),
            draft.index.saturating_add(1)
        ),
        126.0,
        49.0,
        ui::INK,
        85,
    );
    {
        for (index, (field, value)) in draft
            .fields
            .iter()
            .enumerate()
            .skip(draft.page.saturating_mul(6))
            .take(6)
        {
            let row = u16::try_from(index % 6).unwrap_or(0);
            let y = f32::from(row).mul_add(18.0, 66.0);
            let label = if draft.tool == Tool::Route {
                field
                    .path
                    .split('/')
                    .nth(2)
                    .and_then(|part| part.parse::<usize>().ok())
                    .map_or_else(
                        || field.label.to_owned(),
                        |step| format!("Step {} / {}", step.saturating_add(1), field.label),
                    )
            } else {
                field.label.to_owned()
            };
            ui::text(v, &label, 126.0, y + 3.0, ui::INK, 43);
            ui::popup_button(
                v,
                hits,
                [290.0, y, 166.0, 14.0],
                &format!(
                    "{}{value}",
                    if draft.active == Some(index) {
                        "> "
                    } else {
                        ""
                    }
                ),
                Action::Author(Command::Field(index)),
            );
        }
    }
    if draft.tool == Tool::Route && !draft.preview {
        for (x, label, kind) in [
            (126.0, "+ Move", "move"),
            (211.0, "+ Wait", "wait"),
            (296.0, "+ Pose", "play"),
            (381.0, "- Last", "remove"),
        ] {
            ui::popup_button(
                v,
                hits,
                [x, 198.0, 75.0, 12.0],
                label,
                Action::Author(Command::RouteStep(kind)),
            );
        }
    } else {
        ui::text(
            v,
            if matches!(draft.tool, Tool::GlobalPatch | Tool::GlobalRegion) {
                "GLOBAL: affects ALL overlapping storeys."
            } else {
                "Blank = automatic / omitted. Ctrl/Cmd A clears."
            },
            126.0,
            197.0,
            ui::INK,
            90,
        );
    }
    for (x, label, action) in [
        (126.0, "Preview", Action::Author(Command::Preview)),
        (238.0, "Apply", Action::Author(Command::Apply)),
        (350.0, "Cancel", Action::ClosePopup),
    ] {
        ui::popup_button(v, hits, [x, 218.0, 105.0, 13.0], label, action);
    }
    draw_navigation(draft, v, hits);
}

fn draw_hub(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "SPACE AND ENVIRONMENT", 126.0, 50.0, ui::INK, 80);
    for (index, tool) in [
        Tool::Ramp,
        Tool::Stairs,
        Tool::Rail,
        Tool::Water,
        Tool::Sky,
        Tool::Weather,
        Tool::Sun,
        Tool::Ceiling,
        Tool::LocalFloor,
        Tool::GlobalPatch,
        Tool::GlobalRegion,
        Tool::Route,
    ]
    .into_iter()
    .enumerate()
    {
        let x = f32::from(u16::try_from(index % 2).unwrap_or(0)).mul_add(170.0, 126.0);
        let y = f32::from(u16::try_from(index / 2).unwrap_or(0)).mul_add(20.0, 70.0);
        ui::popup_button(
            v,
            hits,
            [x, y, 160.0, 18.0],
            tool.title(),
            Action::Author(Command::Open(tool)),
        );
    }
    ui::text(
        v,
        "Select a room for ceiling or local floor overrides.",
        126.0,
        205.0,
        ui::INK,
        90,
    );
    ui::popup_button(
        v,
        hits,
        [350.0, 218.0, 105.0, 13.0],
        "Close",
        Action::ClosePopup,
    );
}

fn draw_navigation(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if draft.page > 0 {
        ui::popup_button(
            v,
            hits,
            [126.0, 180.0, 48.0, 12.0],
            "< Page",
            Action::Author(Command::Page(false)),
        );
    }
    if draft.page.saturating_add(1).saturating_mul(6) < draft.fields.len() {
        ui::popup_button(
            v,
            hits,
            [180.0, 180.0, 48.0, 12.0],
            "Page >",
            Action::Author(Command::Page(true)),
        );
    }
    if !draft.tool.singleton() && !matches!(draft.tool, Tool::Ceiling | Tool::LocalFloor) {
        for (x, label, action) in [
            (238.0, "<", Action::Author(Command::Record(false))),
            (270.0, ">", Action::Author(Command::Record(true))),
            (302.0, "New", Action::Author(Command::New)),
        ] {
            ui::popup_button(v, hits, [x, 180.0, 30.0, 12.0], label, action);
        }
    }
    if !matches!(draft.tool, Tool::Ceiling | Tool::LocalFloor) {
        ui::popup_button(
            v,
            hits,
            [350.0, 180.0, 105.0, 12.0],
            "Delete record",
            Action::Author(Command::Delete),
        );
    }
}

fn draw_water_slice(app: &App, draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let Some(doc) = &app.document else { return };
    let Ok(record) = draft.record() else { return };
    let value = |key: &str, default: f32| {
        record
            .get(key)
            .and_then(Value::as_f64)
            .and_then(|n| n.to_string().parse::<f32>().ok())
            .unwrap_or(default)
    };
    let x = value("x", 0.0);
    let z = value("z", 0.0);
    let width = value("width", 2.0 * value("radius", 1.0));
    let depth = value("depth", 2.0 * value("radius", 1.0));
    let surface = value("surface_y", 0.0);
    let bottom = value("bottom_y", surface - 2.0);
    let floor = crate::engine::level::WalkableFloor::from_level(doc.level());
    let (origin, length, across) = if draft.slice_z {
        (z, depth, width.mul_add(0.5, x))
    } else {
        (x, width, depth.mul_add(0.5, z))
    };
    let low = bottom - 1.0;
    let high = surface + 1.0;
    let screen_y = |height: f32| {
        ((height - low) / (high - low))
            .clamp(0.0, 1.0)
            .mul_add(-80.0, 151.0)
    };
    ui::rect(v, [292.0, 42.0, 173.0, 133.0], [0.055, 0.08, 0.1, 0.95]);
    ui::text(
        v,
        &format!(
            "{} slice / Y {bottom:.2}..{surface:.2}",
            if draft.slice_z { "Z" } else { "X" }
        ),
        298.0,
        49.0,
        ui::INK,
        42,
    );
    ui::rect(
        v,
        [
            298.0 + 160.0 / (length + 2.0),
            screen_y(surface),
            160.0 * length / (length + 2.0),
            screen_y(bottom) - screen_y(surface),
        ],
        [0.2, 0.55, 0.85, 0.35],
    );
    for index in 0_u16..160 {
        let axis = origin - 1.0 + (length + 2.0) * f32::from(index) / 160.0;
        let (sx, sz) = if draft.slice_z {
            (across, axis)
        } else {
            (axis, across)
        };
        for room in 0..doc.level().rooms.len() {
            if let Some(height) = floor.height_in_room(room, sx, sz) {
                ui::rect(
                    v,
                    [298.0 + f32::from(index), screen_y(height), 1.0, 1.5],
                    [0.9, 0.82, 0.55, 1.0],
                );
            }
        }
    }
    ui::text(v, "Blue: water; gold: floor", 298.0, 155.0, ui::INK, 42);
    ui::popup_button(
        v,
        hits,
        [298.0, 164.0, 160.0, 10.0],
        "Switch X / Z slice",
        Action::Author(Command::Slice),
    );
}

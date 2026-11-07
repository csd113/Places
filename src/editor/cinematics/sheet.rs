use super::{App, Command, Draft};
use crate::editor::document::cinematics::value_at;
use crate::editor::ui::{self, Action, Hit};
use crate::engine::render::Vertex;
pub(in crate::editor) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::rect(v, [0.0, 38.0, 480.0, 204.0], [0.055, 0.08, 0.11, 1.0]);
    ui::text(v, "ADVANCED / CINEMATICS", 6.0, 42.0, ui::INK, 55);
    ui::popup_button(
        v,
        hits,
        [430.0, 40.0, 44.0, 12.0],
        "Close",
        Action::ClosePopup,
    );
    let Some(d) = &app.cinematics else {
        return;
    };
    setup(app, d, v, hits);
    sequences(app, d, v, hits);
    rows(d, v, hits);
    fields(d, v, hits);
    footer(d, v, hits);
}

fn setup(app: &App, d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (x, width, label, command) in [
        (6.0, 36.0, "New", Command::New),
        (46.0, 42.0, "Apply", Command::Apply),
        (92.0, 43.0, "Remove", Command::Remove),
        (139.0, 76.0, "Capture start", Command::CaptureStart),
        (219.0, 81.0, "Capture endpoint", Command::CaptureEnd),
    ] {
        button(v, hits, [x, 55.0, width, 12.0], label, command);
    }
    ui::text(
        v,
        value_at(&d.sequence, "/id").as_str().unwrap_or("New"),
        306.0,
        57.0,
        ui::INK,
        40,
    );
    let controller = d
        .owner
        .and_then(|id| {
            app.document
                .as_ref()?
                .objects()
                .iter()
                .find(|o| o.id == id)
                .map(|o| o.label.clone())
        })
        .unwrap_or_else(|| "Choose trigger".into());
    button(
        v,
        hits,
        [6.0, 70.0, 155.0, 12.0],
        &format!("Controller: {controller}"),
        Command::Owner,
    );
    button(
        v,
        hits,
        [165.0, 70.0, 170.0, 12.0],
        &format!(
            "End: {}",
            d.destination.as_deref().unwrap_or("Return control")
        ),
        Command::Destination,
    );
    button(
        v,
        hits,
        [339.0, 70.0, 135.0, 12.0],
        if d.once {
            "Once per reset"
        } else {
            "Repeat on exit/reentry"
        },
        Command::Once,
    );
}

fn sequences(app: &App, d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "SEQUENCES (click to edit)", 6.0, 86.0, ui::MUTED, 36);
    if let Some(rows) = app
        .document
        .as_ref()
        .and_then(|doc| value_at(doc.raw(), "/sequences").as_array())
    {
        for (slot, (index, row)) in rows
            .iter()
            .enumerate()
            .filter(|(_, r)| !value_at(r, "/cinematic").is_null())
            .skip(d.list_page.saturating_mul(6))
            .take(6)
            .enumerate()
        {
            button(
                v,
                hits,
                [6.0, 98.0 + line(slot), 110.0, 12.0],
                value_at(row, "/id").as_str().unwrap_or("?"),
                Command::Sequence(index),
            );
        }
    }
    button(
        v,
        hits,
        [6.0, 179.0, 52.0, 12.0],
        "< List",
        Command::ListPage(false),
    );
    button(
        v,
        hits,
        [62.0, 179.0, 54.0, 12.0],
        "List >",
        Command::ListPage(true),
    );
}

fn rows(d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "ORDERED STEPS", 122.0, 86.0, ui::MUTED, 32);
    if let Some(rows) = value_at(&d.sequence, "/steps").as_array() {
        let start = (d.row / 5).saturating_mul(5);
        for (index, row) in rows.iter().enumerate().skip(start).take(5) {
            let label = value_at(row, "/motion/kind")
                .as_str()
                .or_else(|| {
                    if value_at(row, "/action/action") == "run_script" {
                        Some("Script")
                    } else {
                        value_at(row, "/step").as_str()
                    }
                })
                .unwrap_or("?");
            button(
                v,
                hits,
                [122.0, 98.0 + line(index.saturating_sub(start)), 119.0, 12.0],
                &format!(
                    "{} {} {label}",
                    if index == d.row { ">" } else { " " },
                    index.saturating_add(1)
                ),
                Command::Row(index),
            );
        }
        button(
            v,
            hits,
            [122.0, 165.0, 56.0, 12.0],
            "Previous",
            Command::Row(d.row.saturating_sub(1)),
        );
        button(
            v,
            hits,
            [182.0, 165.0, 59.0, 12.0],
            "Next",
            Command::Row(d.row.saturating_add(1).min(rows.len().saturating_sub(1))),
        );
    }
    for (slot, (label, command)) in [
        ("Up", Command::Up),
        ("Down", Command::Down),
        ("Delete", Command::RemoveRow),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            v,
            hits,
            [122.0 + column(slot), 179.0, 37.0, 12.0],
            label,
            command,
        );
    }
    for (slot, label) in ["Move", "Follow", "Orbit", "Wait", "Script"]
        .into_iter()
        .enumerate()
    {
        if let Ok(kind) = u8::try_from(slot) {
            button(
                v,
                hits,
                [6.0 + column(slot), 195.0, 37.0, 12.0],
                label,
                Command::Add(kind),
            );
        }
    }
}

fn fields(d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "FIELDS / world metres; angles in degrees",
        248.0,
        86.0,
        ui::MUTED,
        60,
    );
    for (index, (path, value)) in d
        .fields()
        .iter()
        .enumerate()
        .skip(d.page.saturating_mul(7))
        .take(7)
    {
        // Short display values do not round or rewrite the stored camera pose.
        let display = d
            .sequence
            .pointer(path)
            .and_then(serde_json::Value::as_f64)
            .map_or_else(|| value.clone(), |number| format!("{number:.3}"));
        let text = d
            .edit
            .as_ref()
            .filter(|(p, _)| p == path)
            .map_or(display.as_str(), |(_, text)| text.as_str());
        let label = field_label(path, d.row);
        button(
            v,
            hits,
            [248.0, 98.0 + line(index % 7), 226.0, 12.0],
            &format!("{label}: {text}"),
            Command::Field(index),
        );
    }
    for (slot, (label, command)) in [
        ("< Fields", Command::Page(false)),
        ("Fields >", Command::Page(true)),
        ("Aim", Command::Aim),
        ("Target", Command::Target),
        ("Easing", Command::Easing),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            v,
            hits,
            [248.0 + field_column(slot), 195.0, 42.0, 12.0],
            &if command == Command::Aim {
                format!(
                    "Aim: {}",
                    value_at(&d.sequence, &format!("/steps/{}/aim/kind", d.row))
                        .as_str()
                        .unwrap_or("-")
                )
            } else if command == Command::Easing {
                value_at(&d.sequence, &format!("/steps/{}/motion/easing", d.row))
                    .as_str()
                    .unwrap_or("Easing")
                    .to_owned()
            } else {
                label.to_owned()
            },
            command,
        );
    }
}

fn footer(d: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let row = value_at(&d.sequence, &format!("/steps/{}", d.row));
    button(
        v,
        hits,
        [6.0, 210.0, 235.0, 12.0],
        &format!(
            "Script: {}",
            value_at(row, "/action/script")
                .as_str()
                .unwrap_or("select Script row")
        ),
        Command::Script,
    );
    button(
        v,
        hits,
        [248.0, 210.0, 226.0, 12.0],
        &format!(
            "Handler: {}",
            value_at(row, "/action/handler")
                .as_str()
                .unwrap_or("select exported handler")
        ),
        Command::Handler,
    );
    ui::text(
        v,
        "Build geometry normally. Player trigger via Create. Busy/cancel consumes Once until reset.",
        6.0,
        226.0,
        ui::MUTED,
        130,
    );
    ui::text(
        v,
        "Follow/orbit may cut; offsets are world axes. Test/Stop previews; Build + Play changes levels.",
        6.0,
        235.0,
        ui::MUTED,
        130,
    );
}

fn button(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, rect: [f32; 4], label: &str, command: Command) {
    ui::popup_button(v, hits, rect, label, Action::Cinematic(command));
}
fn line(index: usize) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(0)) * 13.0
}
fn column(index: usize) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(0)) * 40.0
}
fn field_column(index: usize) -> f32 {
    f32::from(u16::try_from(index).unwrap_or(0)) * 46.0
}

fn field_label(path: &str, row: usize) -> String {
    let name = path
        .strip_prefix(&format!("/steps/{row}/"))
        .or_else(|| path.strip_prefix("/cinematic/"))
        .unwrap_or(path);
    name.trim_start_matches('/')
        .split('/')
        .map(|part| match part {
            "0" => "X",
            "1" => "Y",
            "2" => "Z",
            "start" => "Start view",
            "position" => "position",
            "yaw_degrees" => "yaw degrees",
            "pitch_degrees" => "pitch degrees",
            "seconds" => "Duration seconds",
            "motion" => "Camera",
            "aim" => "Look-at",
            "target" => "target",
            "offset" => "world offset",
            "id" => "ID",
            "radius" => "radius metres",
            "height" => "height metres",
            "start_degrees" => "start angle degrees",
            "sweep_degrees" => "sweep degrees",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

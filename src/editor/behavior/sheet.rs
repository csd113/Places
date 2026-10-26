use super::{Action, App, Block, Command, Draft, Hit, Popup};
use crate::editor::ui;
use crate::engine::render::Vertex;
pub(in crate::editor) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if !matches!(app.popup, Some(Popup::Behavior)) {
        return;
    }
    ui::rect(v, [0.0, 38.0, 480.0, 204.0], [0.055, 0.08, 0.11, 1.0]);
    let actor = app
        .behavior
        .as_ref()
        .and_then(|draft| {
            app.document
                .as_ref()?
                .objects()
                .iter()
                .find(|object| object.id == draft.id)
                .map(|object| object.label.as_str())
        })
        .unwrap_or("Select an NPC");
    ui::text(v, &format!("BEHAVIORS / {actor}"), 6.0, 42.0, ui::INK, 70);
    let Some(draft) = &app.behavior else {
        return;
    };
    if draft.runtime.is_some() {
        ui::popup_button(
            v,
            hits,
            [425.0, 40.0, 48.0, 11.0],
            "Close",
            Action::ClosePopup,
        );
        runtime(draft, v, hits);
        return;
    }
    let owner = app
        .document
        .as_ref()
        .and_then(|doc| doc.behavior_script(draft.id));
    ui::text(
        v,
        if owner.is_some() {
            "Execution: Luau"
        } else {
            "Execution: Visual"
        },
        280.0,
        42.0,
        ui::INK,
        45,
    );
    ui::popup_button(
        v,
        hits,
        [425.0, 40.0, 48.0, 11.0],
        "Close",
        Action::ClosePopup,
    );
    for (x, width, label, command) in [
        (6.0, 62.0, "Enable AI", Command::Setup),
        (72.0, 66.0, "Reload scene", Command::Reload),
        (142.0, 44.0, "Check", Command::Check),
        (190.0, 55.0, "Apply", Command::Apply),
        (249.0, 70.0, "Use visual", Command::UseVisual),
        (323.0, 84.0, "New Luau copy", Command::GenerateCopy),
        (411.0, 62.0, "Edit source", Command::EditSource),
    ] {
        ui::button(
            v,
            hits,
            [x, 55.0, width, 12.0],
            label,
            Action::Behavior(command),
            command != Command::EditSource || owner.is_some(),
        );
    }
    blocks(draft, v, hits);
    properties(app, v, hits);
    ui::text(
        v,
        "Routines: ordered, failure stops. Tasks: higher priority wins. Loops remain cancellable.",
        6.0,
        233.0,
        ui::MUTED,
        130,
    );
}
fn properties(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let Some(draft) = &app.behavior else {
        return;
    };
    ui::text(v, "PARAMETERS / world metres", 274.0, 72.0, ui::MUTED, 55);
    if draft.locations {
        if let Some(doc) = &app.document {
            for (index, room) in doc
                .level()
                .rooms
                .iter()
                .enumerate()
                .skip(draft.field_page.saturating_mul(6))
                .take(6)
            {
                let y = f32::from(u16::try_from(index % 6).unwrap_or(0)).mul_add(20.0, 85.0);
                ui::popup_button(
                    v,
                    hits,
                    [274.0, y, 198.0, 17.0],
                    &format!(
                        "Room{} center / Y {:.2}",
                        index.saturating_add(1),
                        room.floor_y
                    ),
                    Action::Behavior(Command::Location(index)),
                );
            }
        }
    } else if let Some(choices) = &draft.choices {
        let current = draft.plan.pointer(&choices.path);
        for (index, value) in choices
            .values
            .iter()
            .enumerate()
            .skip(draft.field_page.saturating_mul(6))
            .take(6)
        {
            let y = f32::from(u16::try_from(index % 6).unwrap_or(0)).mul_add(20.0, 85.0);
            let picked = current.is_some_and(|current_value| {
                current_value.as_str() == Some(value)
                    || current_value
                        .as_array()
                        .is_some_and(|a| a.iter().any(|item| item.as_str() == Some(value)))
            });
            ui::popup_button(
                v,
                hits,
                [274.0, y, 198.0, 17.0],
                &format!("{} {value}", if picked { "[x]" } else { "[ ]" }),
                Action::Behavior(Command::Choice(index)),
            );
        }
        ui::text(
            v,
            "Esc finishes choices; empty = any/none.",
            274.0,
            225.0,
            ui::MUTED,
            56,
        );
    } else {
        fields(draft, v, hits);
    }
    for (x, label, forward) in [(274.0, "Prev fields", false), (376.0, "Next fields", true)] {
        ui::popup_button(
            v,
            hits,
            [x, 208.0, 96.0, 11.0],
            label,
            Action::Behavior(Command::Fields(forward)),
        );
    }
}

fn blocks(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(v, "DRAG OR CLICK", 6.0, 72.0, ui::MUTED, 25);
    for (index, kind) in Block::ALL.into_iter().enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(13.0, 84.0);
        ui::popup_button(
            v,
            hits,
            [6.0, y, 92.0, 11.0],
            kind.title(),
            Action::Behavior(Command::Add(kind)),
        );
    }
    ui::popup_button(
        v,
        hits,
        [6.0, 216.0, 92.0, 11.0],
        if draft
            .plan
            .get("receive_alerts")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            "Receive alerts: on"
        } else {
            "Receive alerts: off"
        },
        Action::Behavior(Command::Alerts),
    );
    ui::text(v, "ORDERED BLOCKS", 104.0, 72.0, ui::MUTED, 39);
    let rows = draft.rows();
    for (index, row) in rows
        .iter()
        .enumerate()
        .skip(draft.row_page.saturating_mul(8))
        .take(8)
    {
        let y = f32::from(u16::try_from(index % 8).unwrap_or(0)).mul_add(15.0, 84.0);
        let x = f32::from(u16::try_from(row.depth).unwrap_or(0)).mul_add(5.0, 104.0);
        let label = format!(
            "{}{}",
            if draft.selected.as_deref() == Some(&row.path) {
                "> "
            } else {
                "  "
            },
            row.label
        );
        ui::popup_button(
            v,
            hits,
            [x, y, 264.0 - x, 13.0],
            &label,
            Action::Behavior(Command::Row(index)),
        );
    }
    if rows.is_empty() {
        ui::text(v, "Add a routine, then actions.", 104.0, 89.0, ui::INK, 40);
    }
    for (x, label, command) in [
        (104.0, "Prev", Command::Rows(false)),
        (156.0, "Next", Command::Rows(true)),
        (208.0, "Remove", Command::Remove),
    ] {
        ui::popup_button(
            v,
            hits,
            [x, 208.0, 48.0, 11.0],
            label,
            Action::Behavior(command),
        );
    }
    ui::popup_button(
        v,
        hits,
        [104.0, 222.0, 160.0, 10.0],
        "Inspect last Play >",
        Action::Behavior(Command::Runtime),
    );
}

fn fields(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (index, field) in draft
        .fields()
        .iter()
        .enumerate()
        .skip(draft.field_page.saturating_mul(6))
        .take(6)
    {
        let y = f32::from(u16::try_from(index % 6).unwrap_or(0)).mul_add(20.0, 83.0);
        ui::text(v, &field.label, 274.0, y, ui::MUTED, 56);
        let value = draft
            .edit
            .as_ref()
            .filter(|(path, _)| path == &field.path)
            .map_or_else(
                || {
                    field
                        .value
                        .as_str()
                        .map_or_else(|| field.value.to_string(), str::to_owned)
                },
                |(_, text)| format!("> {text}"),
            );
        ui::popup_button(
            v,
            hits,
            [274.0, y + 8.0, 198.0, 11.0],
            &value,
            Action::Behavior(Command::Field(index)),
        );
    }
    let is_rule = draft
        .selected
        .as_deref()
        .and_then(|p| draft.plan.pointer(p))
        .is_some_and(|value| value.get("filter").is_some());
    if is_rule {
        for (x, label, kind) in [
            (274.0, "Search", 0),
            (340.0, "Routine", 1),
            (406.0, "Alert", 2),
        ] {
            ui::popup_button(
                v,
                hits,
                [x, 224.0, 62.0, 9.0],
                label,
                Action::Behavior(Command::Response(kind)),
            );
        }
    } else {
        ui::popup_button(
            v,
            hits,
            [376.0, 224.0, 96.0, 9.0],
            "Room location >",
            Action::Behavior(Command::Locations),
        );
    }
}

fn runtime(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    ui::text(
        v,
        "REAL PLAYER SNAPSHOT / Refresh reads the latest sample",
        6.0,
        56.0,
        ui::INK,
        115,
    );
    for (index, line) in draft
        .runtime
        .iter()
        .flatten()
        .skip(draft.runtime_page.saturating_mul(20))
        .take(20)
        .enumerate()
    {
        ui::text(
            v,
            line,
            6.0,
            f32::from(u16::try_from(index).unwrap_or(0)).mul_add(7.5, 72.0),
            ui::INK,
            115,
        );
    }
    for (x, label, command) in [
        (6.0, "< Blocks", Command::RuntimeBack),
        (126.0, "Refresh", Command::Runtime),
        (246.0, "Previous", Command::RuntimePage(false)),
        (366.0, "Next", Command::RuntimePage(true)),
    ] {
        ui::popup_button(
            v,
            hits,
            [x, 225.0, 108.0, 12.0],
            label,
            Action::Behavior(command),
        );
    }
}

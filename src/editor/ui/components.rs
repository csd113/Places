//! Advanced capabilities use a section hub and bounded forms, leaving placement uncluttered.

mod events;
mod fields;

use super::{ACCENT, Action, Hit, INK, MUTED, button, text};
use crate::editor::App;
use crate::editor::document::ObjectId;
use crate::editor::document::components::{Field, Kind, Target};
use crate::engine::render::Vertex;
use serde_json::Value;

pub(in crate::editor) fn draw(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    page: usize,
) {
    let section = page % 100;
    let offset = page / 100;
    let title = match section {
        1 => "TYPED STATE",
        2 => "EVENTS",
        3 => "EVENT CONDITIONS",
        4 => "EVENT ACTIONS",
        5 => "LIGHT CONTROLLER",
        6 => "PHYSICS DETAILS",
        7 => "AI AND BEAM RESPONSE",
        8 => "IDENTITY / INTERACTION",
        9 => "ATTACHED AUDIO",
        10 => "MAP SETTINGS",
        11 => "CHOOSE STATE CONDITION",
        12 => "ATTACHED SOURCE",
        _ => "CAPABILITIES",
    };
    text(vertices, title, 140.0, 73.0, ACCENT, 62);
    let Some(doc) = app.document.as_ref() else {
        return;
    };
    let id = app.selected.unwrap_or(ObjectId::Prop(0));
    if section != 0 {
        button(
            vertices,
            hits,
            [140.0, 83.0, 55.0, 10.0],
            "< Sections",
            Action::Components(0),
            true,
        );
    }
    match section {
        0 => hub(app, vertices, hits),
        1 => states(app, vertices, hits, offset),
        2..=4 | 11 => events::draw(app, vertices, hits, section, offset),
        12 => source(app, vertices, hits, offset),
        _ => {
            let object = doc.component_object(id);
            let list = match section {
                5 => fields::lighting(matches!(id, ObjectId::Light(_))),
                6 => fields::physics(),
                7 => fields::ai(),
                8 => fields::identity(),
                9 => fields::audio(),
                10 => fields::map(),
                _ => Vec::new(),
            };
            let visible_fields: Vec<_> = list
                .into_iter()
                .filter(|field| match field.target {
                    Target::Component(kind) => has_component(object, kind),
                    Target::Physics => object.is_some_and(|record| record.get("physics").is_some()),
                    Target::Identity
                    | Target::Map
                    | Target::Spawn
                    | Target::Defaults
                    | Target::Presentation
                    | Target::Fog
                    | Target::Object
                    | Target::Filter
                    | Target::NewState
                    | Target::State(_)
                    | Target::Light(_)
                    | Target::Binding(_)
                    | Target::Condition(_, _) => true,
                })
                .collect();
            form(app, vertices, hits, &visible_fields, section, offset);
            if section == 6 && matches!(id, ObjectId::Prop(_)) {
                button(
                    vertices,
                    hits,
                    [200.0, 83.0, 180.0, 10.0],
                    "Use fitted catalog fixed shape",
                    Action::CatalogFixedShape,
                    true,
                );
            }
            if section == 5 && matches!(id, ObjectId::Prop(_)) {
                button(
                    vertices,
                    hits,
                    [263.0, 83.0, 117.0, 10.0],
                    "Attached sources >",
                    Action::Components(12),
                    true,
                );
            }
        }
    }
    button(
        vertices,
        hits,
        [330.0, 196.0, 50.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

fn hub(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let supported = matches!(
        app.selected,
        Some(
            ObjectId::Prop(_)
                | ObjectId::Door(_)
                | ObjectId::Trigger(_)
                | ObjectId::Light(_)
                | ObjectId::Audio(_)
        )
    );
    for (index, (label, page)) in [
        ("Identity / interaction", 8),
        ("Typed state", 1),
        ("Events / scripts", 2),
        ("Light controller", 5),
        ("Physics details", 6),
        ("AI / beam response", 7),
        ("Attached audio", 9),
        ("Map settings", 10),
    ]
    .into_iter()
    .enumerate()
    {
        let row = u16::try_from(index / 2).unwrap_or(0);
        button(
            vertices,
            hits,
            [
                if index % 2 == 0 { 140.0 } else { 263.0 },
                f32::from(row).mul_add(14.0, 88.0),
                117.0,
                12.0,
            ],
            label,
            Action::Components(page),
            page == 10
                || (supported
                    && match page {
                        1 | 9 => {
                            matches!(app.selected, Some(ObjectId::Prop(_) | ObjectId::Door(_)))
                        }
                        5 => matches!(app.selected, Some(ObjectId::Prop(_) | ObjectId::Light(_))),
                        6 | 7 => matches!(app.selected, Some(ObjectId::Prop(_))),
                        _ => true,
                    }),
        );
    }
    if let Some((doc, id)) = app.document.as_ref().zip(app.selected) {
        let object = doc.component_object(id);
        if matches!(id, ObjectId::Prop(_) | ObjectId::Door(_)) {
            for (index, (kind, label)) in [
                ("interactable", "Interaction"),
                ("light", "Light"),
                ("light_response", "Beam"),
                ("audio", "Audio"),
            ]
            .into_iter()
            .enumerate()
            {
                let exists = has_component(object, kind);
                button(
                    vertices,
                    hits,
                    [
                        f32::from(u16::try_from(index).unwrap_or(0)).mul_add(61.0, 140.0),
                        154.0,
                        57.0,
                        11.0,
                    ],
                    &format!("{} {label}", if exists { "-" } else { "+" }),
                    if exists {
                        Action::ComponentRemove(kind)
                    } else {
                        Action::ComponentAdd(kind)
                    },
                    true,
                );
            }
        }
    }
    text(
        vertices,
        "Add/remove validates dependent events before changing source.",
        140.0,
        174.0,
        MUTED,
        68,
    );
}

fn has_component(record: Option<&Value>, kind: &str) -> bool {
    record
        .and_then(|object| object.get("components"))
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.get("component").and_then(Value::as_str) == Some(kind))
        })
}

fn form(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    fields: &[Field],
    section: usize,
    offset: usize,
) {
    for (row, field) in fields
        .iter()
        .skip(offset.saturating_mul(6))
        .take(6)
        .enumerate()
    {
        field_row(app, v, hits, *field, row, 97.0, 240.0);
    }
    if fields.len() > 6 {
        pagination(v, hits, section, offset, fields.len().div_ceil(6));
    }
}

pub(super) fn field_row(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    field: Field,
    row: usize,
    start: f32,
    width: f32,
) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };
    let id = app.selected.unwrap_or(ObjectId::Prop(0));
    let value = if field.target == Target::Binding(app.selected_binding())
        && field.key == "on"
        && app.selected_binding() >= doc.binding_count(id)
    {
        app.selected_event().name().into()
    } else {
        doc.component_value(id, field)
    };
    let display = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_string);
    button(
        v,
        hits,
        [
            140.0,
            f32::from(u16::try_from(row).unwrap_or(0)).mul_add(13.0, start),
            width,
            11.0,
        ],
        &format!("{}: {display}", field.label),
        Action::ComponentField(field),
        true,
    );
}

fn states(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, offset: usize) {
    let Some((doc, id)) = app.document.as_ref().zip(app.selected) else {
        return;
    };
    let entries: Vec<_> = doc
        .component_object(id)
        .and_then(|object| object.get("components"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter(|(_, entry)| entry.get("component").and_then(Value::as_str) == Some("state"))
        .collect();
    for (row, (index, _entry)) in entries
        .iter()
        .skip(offset.saturating_mul(3))
        .take(3)
        .enumerate()
    {
        let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(27.0, 97.0);
        field_row(
            app,
            v,
            hits,
            Field::new(Target::State(*index), "name", "Name", Kind::Text, "\"\""),
            0,
            y,
            206.0,
        );
        let field = Field::new(
            Target::State(*index),
            "value",
            "Value",
            Kind::State,
            "false",
        );
        field_row(app, v, hits, field, 0, y + 12.0, 206.0);
        button(
            v,
            hits,
            [350.0, y, 30.0, 11.0],
            "-",
            Action::StateRemove(*index),
            true,
        );
    }
    button(
        v,
        hits,
        [263.0, 83.0, 117.0, 10.0],
        "+ Bool state",
        Action::StateAdd,
        matches!(id, ObjectId::Prop(_) | ObjectId::Door(_)),
    );
    pagination(v, hits, 1, offset, entries.len().div_ceil(3));
}

fn source(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, index: usize) {
    let count = app
        .document
        .as_ref()
        .zip(app.selected)
        .and_then(|(doc, id)| doc.component_object(id))
        .and_then(|object| object.get("lights"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    button(
        v,
        hits,
        [200.0, 83.0, 85.0, 10.0],
        "+ Point source",
        Action::AttachedLightAdd,
        count < 16,
    );
    if index < count {
        for (row, field) in fields::source(index).into_iter().enumerate() {
            field_row(app, v, hits, field, row, 97.0, 240.0);
        }
        button(
            v,
            hits,
            [289.0, 83.0, 91.0, 10.0],
            "Remove source",
            Action::AttachedLightRemove(index),
            true,
        );
    } else {
        text(
            v,
            "Add a source, or select a previous source.",
            140.0,
            110.0,
            MUTED,
            68,
        );
    }
    pagination(v, hits, 12, index, count);
}

pub(super) fn pagination(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    section: usize,
    offset: usize,
    count: usize,
) {
    for (x, label, next, enabled) in [
        (140.0, "< Previous", offset.saturating_sub(1), offset > 0),
        (
            205.0,
            "Next >",
            offset.saturating_add(1),
            offset.saturating_add(1) < count,
        ),
    ] {
        button(
            v,
            hits,
            [x, 182.0, 60.0, 10.0],
            label,
            Action::Components(next.saturating_mul(100).saturating_add(section)),
            enabled,
        );
    }
}

pub(in crate::editor) fn draw_text(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    field: Field,
    value: &str,
) {
    text(v, field.label, 140.0, 75.0, ACCENT, 65);
    text(v, value, 140.0, 98.0, INK, 68);
    text(
        v,
        match field.kind {
            Kind::Vector => "Three numbers in brackets: [x, y, z]",
            Kind::State => "true / false / integer / decimal / quoted text",
            Kind::Text => "Empty clears an optional value. Cmd/Ctrl A clears.",
            Kind::Number | Kind::Bool | Kind::Choice(_) => {
                "Enter a finite number. Cmd/Ctrl A clears."
            }
        },
        140.0,
        121.0,
        MUTED,
        68,
    );
    text(v, "Enter applies / Esc cancels", 140.0, 137.0, MUTED, 68);
    button(
        v,
        hits,
        [140.0, 164.0, 80.0, 12.0],
        "Apply",
        Action::ComponentApply,
        true,
    );
}

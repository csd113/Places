//! First-class elevator setup: ordinary fields, installed destinations and one undo.
use super::{App, Command, Draft, Field, Hit, Kind, Tool, Value, Vertex, number, text, ui};
use crate::editor::document::{Document, ObjectId};

pub(super) fn fields(record: Option<&Value>) -> Vec<Field> {
    let mut result = vec![
        text("/id", "Unique elevator ID", "lift"),
        number("/x", "Door centre X / world m", "0"),
        number("/z", "Door centre Z / world m", "0"),
        number("/rotation_degrees", "Yaw / cabin extends local +Z", "0"),
        Field {
            kind: Kind::InitialStop,
            ..number("/initial_stop", "Start at / click to choose", "0")
        },
        number("/speed_mps", "Travel speed m/s (0.25..4)", "1.5"),
    ];
    let count = record
        .and_then(|r| r.get("stops"))
        .and_then(Value::as_array)
        .map_or(2, Vec::len);
    for stop in 0..count {
        for (key, label, kind, default) in [
            ("label", "Button label", Kind::Text, "Floor"),
            ("level", "Destination / click to choose", Kind::Level, ""),
            ("y", "Floor elevation / this map m", Kind::Number, "0"),
            (
                "arrival_elevator",
                "Arrival lift / click to choose",
                Kind::Arrival,
                "",
            ),
        ] {
            result.push(Field {
                path: format!("/stops/{stop}/{key}").into(),
                label,
                kind,
                default,
            });
        }
    }
    result
}

pub(super) fn new_record(app: &App) -> Value {
    let [x, _, z] = app.viewport.target();
    let doc = app.document.as_ref();
    let floor = doc
        .and_then(|document| match app.selected {
            Some(ObjectId::Room(i)) => document.level().rooms.get(i),
            _ => None,
        })
        .map_or(0.0, |room| room.floor_y);
    let mut number = 1_usize;
    let id = loop {
        let id = format!("lift_{number}");
        if doc.is_none_or(|document| document.level().elevators.iter().all(|lift| lift.id != id)) {
            break id;
        }
        number = number.saturating_add(1);
    };
    serde_json::json!({"id":id,"x":x,"z":z,"stops":[{"label":"G","y":floor},{"label":"1","y":floor+3.5}]})
}

pub(super) fn normalize(record: &Value) -> Result<(), String> {
    if record.get("stops").and_then(Value::as_array).is_none() {
        return Err("Add a destination".into());
    }
    Ok(())
}

pub(super) fn link_destinations(
    doc: &Document,
    record: &Value,
    raw: &mut Value,
) -> Result<(), String> {
    let stops = record
        .get("stops")
        .and_then(Value::as_array)
        .ok_or("Add a destination")?;
    for stop in stops {
        let Some(level) = stop.get("level").and_then(Value::as_str) else {
            continue;
        };
        if level == doc.level_id() {
            return Err("Choose 'This map / floor' for local travel".into());
        }
        let (_, _, target) = doc.project().read_level(Some(level))?;
        let arrival = stop
            .get("arrival_elevator")
            .and_then(Value::as_str)
            .or_else(|| record.get("id").and_then(Value::as_str))
            .unwrap_or("");
        if target.elevators.iter().all(|lift| lift.id != arrival) {
            return Err(format!(
                "Create arrival elevator '{arrival}' in {level} first"
            ));
        }
        let object = raw.as_object_mut().ok_or("Invalid level")?;
        let links = object
            .entry("transitions")
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .ok_or("Invalid transitions")?;
        if !links.iter().any(|id| id.as_str() == Some(level)) {
            links.push(Value::String(level.into()));
        }
    }
    Ok(())
}

pub(super) fn choose(doc: &Document, draft: &mut Draft, index: usize) -> Result<bool, String> {
    if draft.tool != Tool::Elevator {
        return Ok(false);
    }
    let Some((field, current)) = draft.fields.get(index) else {
        return Ok(false);
    };
    let record = draft.record()?;
    let mut choices = Vec::new();
    match field.kind {
        Kind::Level => {
            choices.push(String::new());
            choices.extend(
                doc.level_ids()
                    .filter(|id| *id != doc.level_id())
                    .map(str::to_owned),
            );
        }
        Kind::Arrival => {
            let level_path = field.path.replace("arrival_elevator", "level");
            if let Some(level) = record.pointer(&level_path).and_then(Value::as_str) {
                let (_, _, target) = doc.project().read_level(Some(level))?;
                choices.extend(target.elevators.iter().map(|lift| lift.id.clone()));
            } else {
                choices.push(String::new());
                choices.extend(
                    doc.level()
                        .elevators
                        .iter()
                        .filter(|lift| record.get("id").and_then(Value::as_str) != Some(&lift.id))
                        .map(|lift| lift.id.clone()),
                );
            }
            if choices.is_empty() {
                return Err("Place and save an arrival elevator in that level first".into());
            }
        }
        Kind::InitialStop => {
            if let Some(stops) = record.get("stops").and_then(Value::as_array) {
                choices.extend(
                    stops
                        .iter()
                        .enumerate()
                        .filter(|(_, stop)| {
                            stop.get("level").is_none() && stop.get("arrival_elevator").is_none()
                        })
                        .map(|(i, _)| i.to_string()),
                );
            }
        }
        Kind::Json | Kind::Text | Kind::Number | Kind::Bool => return Ok(false),
    }
    let next = choices
        .iter()
        .position(|choice| choice == current)
        .map_or(0, |i| {
            i.saturating_add(1).checked_rem(choices.len()).unwrap_or(0)
        });
    if let Some(value) = choices.get(next)
        && let Some((_, selected)) = draft.fields.get_mut(index)
    {
        selected.clone_from(value);
    }
    draft.active = None;
    draft.preview = false;
    Ok(true)
}

pub(super) fn edit_stop(draft: &mut Draft, add: bool) -> Result<(), String> {
    if draft.tool != Tool::Elevator {
        return Ok(());
    }
    let mut record = draft.record()?;
    let initial = record
        .get("initial_stop")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let stops = record
        .get_mut("stops")
        .and_then(Value::as_array_mut)
        .ok_or("Invalid destinations")?;
    if add {
        if stops.len() >= crate::engine::elevator::MAX_STOPS {
            return Err(
                "At most eight physical shaft stops; linked groups support three cabins".into(),
            );
        }
        let y = stops
            .iter()
            .filter(|stop| stop.get("level").is_none() && stop.get("arrival_elevator").is_none())
            .filter_map(|stop| stop.get("y").and_then(Value::as_f64))
            .fold(0.0_f64, f64::max)
            + 3.5_f64;
        stops.push(serde_json::json!({"label":format!("{}",stops.len()),"y":y}));
        draft.page = stops.len();
    } else {
        let index = draft.page.saturating_sub(1);
        if draft.page == 0 || stops.len() <= 1 || u64::try_from(index).ok() == Some(initial) {
            return Err(
                "Keep the initial local landing; select another destination to remove".into(),
            );
        }
        drop(stops.remove(index));
        draft.page = draft.page.min(stops.len());
        if u64::try_from(index).is_ok_and(|removed| removed < initial) {
            super::set_path(
                &mut record,
                "/initial_stop",
                Some(Value::from(initial.saturating_sub(1))),
            )?;
        }
    }
    draft.fields = fields(Some(&record))
        .into_iter()
        .map(|field| {
            let value = record.pointer(&field.path).map_or_else(
                || field.default.into(),
                |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
            );
            (field, value)
        })
        .collect();
    draft.original = Some(record);
    draft.active = None;
    draft.preview = false;
    Ok(())
}

pub(super) fn draw(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let record = draft.record().unwrap_or(Value::Null);
    let count = record
        .get("stops")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let start = if draft.page == 0 {
        0
    } else {
        6_usize.saturating_add(draft.page.saturating_sub(1).saturating_mul(4))
    };
    let take = if draft.page == 0 { 6 } else { 4 };
    for (row, (index, (field, value))) in draft
        .fields
        .iter()
        .enumerate()
        .skip(start)
        .take(take)
        .enumerate()
    {
        let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(18.0, 66.0);
        let label = match field.kind {
            Kind::Level if value.is_empty() => "This map".into(),
            Kind::Arrival if value.is_empty() => {
                if record
                    .pointer(&field.path.replace("arrival_elevator", "level"))
                    .is_some()
                {
                    "Same elevator ID (auto)".into()
                } else {
                    "None / floor landing".into()
                }
            }
            Kind::InitialStop => value
                .parse::<usize>()
                .ok()
                .and_then(|i| record.pointer(&format!("/stops/{i}/label")))
                .and_then(Value::as_str)
                .unwrap_or(value)
                .to_owned(),
            Kind::Json | Kind::Number | Kind::Text | Kind::Bool | Kind::Level | Kind::Arrival => {
                value.clone()
            }
        };
        ui::text(v, field.label, 126.0, y + 3.0, ui::INK, 43);
        ui::popup_button(
            v,
            hits,
            [290.0, y, 166.0, 14.0],
            &format!(
                "{}{label}",
                if draft.active == Some(index) {
                    "> "
                } else {
                    ""
                }
            ),
            super::Action::Author(Command::Field(index)),
        );
    }
    draw_navigation(draft, count, v, hits);
}

fn draw_navigation(draft: &Draft, count: usize, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if draft.page > 0 {
        ui::text(
            v,
            &format!(
                "Destination {} of {count} / buttons shown in order",
                draft.page
            ),
            126.0,
            143.0,
            ui::INK,
            85,
        );
        for (x, label, add) in [
            (126.0, "+ Destination", true),
            (296.0, "Remove destination", false),
        ] {
            ui::popup_button(
                v,
                hits,
                [x, 159.0, 160.0, 13.0],
                label,
                super::Action::Author(Command::ElevatorStop(add)),
            );
        }
    }
    if draft.page > 0 {
        ui::popup_button(
            v,
            hits,
            [126.0, 180.0, 100.0, 12.0],
            "< Back",
            super::Action::Author(Command::Page(false)),
        );
    }
    if draft.page < count {
        ui::popup_button(
            v,
            hits,
            [238.0, 180.0, 100.0, 12.0],
            "Destinations >",
            super::Action::Author(Command::Page(true)),
        );
    }
    ui::popup_button(
        v,
        hits,
        [350.0, 180.0, 51.0, 12.0],
        "< Lift",
        super::Action::Author(Command::Record(false)),
    );
    ui::popup_button(
        v,
        hits,
        [405.0, 180.0, 51.0, 12.0],
        "Lift >",
        super::Action::Author(Command::Record(true)),
    );
    if draft.page == 0 {
        ui::popup_button(
            v,
            hits,
            [350.0, 197.0, 105.0, 12.0],
            "Delete elevator",
            super::Action::Author(Command::Delete),
        );
    }
    ui::text(
        v,
        if draft.page == 0 {
            "Shaft 2.6 x 2.5; opening 1.5 x 2.25 m."
        } else {
            "Separate level: choose project level + arrival lift. Links auto-save."
        },
        126.0,
        197.0,
        ui::INK,
        90,
    );
}

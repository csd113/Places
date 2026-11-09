//! Native creature drafts retain one complete prop transaction and existing validation.
use super::{App, Command, Draft, Field, Hit, Kind, Tool, Value, Vertex, flag, number, text, ui};
use crate::editor::document::Document;
use serde_json::json;

pub(super) const fn native(tool: Tool) -> bool {
    matches!(tool, Tool::Feltling | Tool::TallGuest)
}
pub(super) const fn key(tool: Tool) -> &'static str {
    if matches!(tool, Tool::Feltling) {
        "feltling"
    } else {
        "tall_guest"
    }
}

pub(super) fn initialize(
    _app: &App,
    tool: Tool,
    original: &mut Option<Value>,
) -> Result<(), String> {
    if !native(tool) {
        return Ok(());
    }
    let record = original
        .as_mut()
        .ok_or("Select a creature catalog instance")?;
    let model = record
        .get("model")
        .and_then(Value::as_str)
        .ok_or("Missing model")?;
    if super::creature_tool(model) != Some(tool) {
        return Err("Select the matching native creature catalog model".into());
    }
    let prop = record.as_object_mut().ok_or("Invalid creature")?;
    if prop.get(key(tool)).is_none_or(Value::is_null) {
        drop(prop.insert(
            key(tool).into(),
            if tool == Tool::Feltling {
                json!({"mode":"wait","routes":[]})
            } else {
                json!({"patrol":[],"prey_targets":[],"passages":[]})
            },
        ));
        // This visible field is part of the reviewed Apply transaction. No other
        // motion/physics owner is removed; full admission refuses conflicts.
        drop(prop.insert("solid".into(), json!(false)));
    }
    clean_optional(record, key(tool))?;
    Ok(())
}
fn clean_optional(record: &mut Value, name: &str) -> Result<(), String> {
    let definition = record
        .get_mut(name)
        .and_then(Value::as_object_mut)
        .ok_or("Invalid native definition")?;
    for field in ["look_at", "guide_recovery"] {
        if definition.get(field).is_some_and(Value::is_null) {
            drop(definition.remove(field));
        }
    }
    if let Some(passages) = definition.get_mut("passages").and_then(Value::as_array_mut) {
        for passage in passages {
            if passage.get("protected").is_some_and(Value::is_null) {
                drop(
                    passage
                        .as_object_mut()
                        .ok_or("Invalid passage")?
                        .remove("protected"),
                );
            }
        }
    }
    Ok(())
}
fn dynamic(path: String, label: &'static str, kind: Kind, default: &'static str) -> Field {
    Field {
        path: path.into(),
        label,
        kind,
        default,
    }
}
fn vector(result: &mut Vec<Field>, path: &str, labels: [&'static str; 3], optional: bool) {
    for (axis, label) in labels.into_iter().enumerate() {
        result.push(dynamic(
            format!("{path}/{axis}"),
            label,
            Kind::Number,
            if optional { "" } else { "0" },
        ));
    }
}
pub(super) fn fields(tool: Tool, record: Option<&Value>) -> Vec<Field> {
    let mut result = vec![flag("/solid", "Solid / native requires false", "false")];
    let root = format!("/{}", key(tool));
    let definition = record.and_then(|v| v.get(key(tool)));
    if tool == Tool::Feltling {
        feltling_fields(&mut result, &root, definition);
    } else {
        guest_fields(&mut result, &root, definition);
    }

    result
}
fn point_fields(result: &mut Vec<Field>, path: &str, points: Option<&Value>) {
    if let Some(entries) = points.and_then(Value::as_array) {
        for index in 0..entries.len() {
            vector(
                result,
                &format!("{path}/{index}"),
                [
                    "Feet X / world metres",
                    "Feet Y / world metres",
                    "Feet Z / world metres",
                ],
                false,
            );
        }
    }
}
fn null_tree(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(a) => a.iter().all(null_tree),
        Value::Object(o) => o.values().all(null_tree),
        Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}
pub(super) fn normalize(tool: Tool, record: &mut Value) -> Result<(), String> {
    let d = record
        .get_mut(key(tool))
        .and_then(Value::as_object_mut)
        .ok_or("Missing native definition")?;
    for name in ["look_at", "guide_recovery"] {
        if d.get(name).is_some_and(null_tree) {
            drop(d.remove(name));
        }
    }
    if let Some(passages) = d.get_mut("passages").and_then(Value::as_array_mut) {
        for passage in passages {
            if passage.get("protected").is_some_and(null_tree) {
                drop(
                    passage
                        .as_object_mut()
                        .ok_or("Invalid passage")?
                        .remove("protected"),
                );
            }
        }
    }
    // Runtime owns pose/navigation; never silently discard competing data.
    if record
        .get("components")
        .and_then(Value::as_array)
        .is_some_and(|c| {
            c.iter().any(|v| {
                matches!(
                    v.get("component").and_then(Value::as_str),
                    Some("ai" | "animation" | "nav_agent")
                )
            })
        })
    {
        return Err(
            "Remove competing AI / animation / navigation owner before enabling native creature"
                .into(),
        );
    }
    Ok(())
}
fn refill(draft: &mut Draft, record: Value) {
    draft.fields = fields(draft.tool, Some(&record))
        .into_iter()
        .map(|f| {
            let value = record.pointer(&f.path).map_or_else(
                || f.default.into(),
                |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
            );
            (f, value)
        })
        .collect();
    draft.original = Some(record);
    draft.active = None;
    draft.preview = false;
    draft.native_cursor = draft
        .native_cursor
        .min(draft.fields.len().saturating_sub(1));
    draft.page = draft.native_cursor / 6;
}
fn selected_path(draft: &Draft) -> String {
    draft
        .fields
        .get(draft.native_cursor)
        .map_or_else(String::new, |(f, _)| f.path.to_string())
}
fn native_ids(doc: &Document) -> Vec<String> {
    doc.level()
        .props
        .iter()
        .zip(doc.level().prop_instance_ids())
        .filter_map(|(p, id)| p.feltling.as_ref().map(|_| id))
        .collect()
}
pub(super) fn choose(doc: &Document, draft: &mut Draft, index: usize) -> Result<bool, String> {
    if !native(draft.tool) {
        return Ok(false);
    }
    let Some((f, value)) = draft.fields.get_mut(index) else {
        return Ok(false);
    };
    if draft.tool == Tool::TallGuest {
        if f.path.contains("/patrol/") {
            draft.native_group = "patrol";
        } else if f.path.contains("/prey_targets/") {
            draft.native_group = "prey_targets";
        } else if f.path.contains("/passages/") {
            draft.native_group = "passages";
        }
    }
    if !f.path.contains("/prey_targets/") {
        return Ok(false);
    }
    let ids = native_ids(doc);
    let next = ids.iter().position(|id| id == value).map_or(0, |i| {
        i.saturating_add(1)
            .checked_rem(ids.len().max(1))
            .unwrap_or(0)
    });
    *value = ids
        .get(next)
        .cloned()
        .ok_or("Add a native Feltling first")?;
    draft.active = None;
    draft.preview = false;
    Ok(true)
}
fn choose_group(draft: &mut Draft, operation: &str) -> bool {
    let group = match operation {
        "patrol" => "patrol",
        "prey" => "prey_targets",
        "passage" => "passages",
        _ => return false,
    };
    draft.native_group = group;
    let root = format!("/{}/{}", key(draft.tool), group);
    if let Some(cursor) = draft
        .fields
        .iter()
        .position(|(f, _)| f.path.starts_with(&root))
    {
        draft.native_cursor = cursor;
        draft.page = cursor / 6;
    }
    true
}
fn new_row(doc: &Document, group: &str, point: bool, list: &[Value]) -> Result<Value, String> {
    let limit = if point || group == "patrol" {
        32
    } else if group == "routes" {
        8
    } else {
        16
    };
    if list.len() >= limit {
        return Err(format!("This list allows at most {limit} entries"));
    }
    if point || group == "patrol" {
        return Ok(json!([0_f32, 0_f32, 0_f32]));
    }
    if group == "prey_targets" {
        let id = native_ids(doc)
            .into_iter()
            .find(|id| !list.iter().any(|v| v.as_str() == Some(id)))
            .ok_or("No unused native Feltling; add one first")?;
        return Ok(json!(id));
    }
    // Generate only a unique row label; no source mutation until reviewed Apply.
    let mut serial = list.len().saturating_add(1);
    let name = loop {
        let proposed = format!("{group}_{serial}");
        if !list
            .iter()
            .any(|v| v.get("id").and_then(Value::as_str) == Some(&proposed))
        {
            break proposed;
        }
        serial = serial.saturating_add(1);
    };
    Ok(if group == "routes" {
        json!({"id":name,"points":[[0_f32,0_f32,0_f32]]})
    } else {
        json!({"id":name,"entry":[0_f32,0_f32,0_f32],"exit":[0_f32,0_f32,1_f32],"crouch_only":false})
    })
}
fn mutate_list(
    doc: &Document,
    list: &mut Vec<Value>,
    group: &str,
    point: bool,
    index: usize,
    operation: &str,
) -> Result<(), String> {
    match operation {
        "add" | "point_add" => list.push(new_row(doc, group, point, list)?),
        "remove" | "point_remove" => {
            if index >= list.len() {
                return Err("Select an existing list row".into());
            }
            drop(list.remove(index));
        }
        "up" | "down" => {
            let adjacent = if operation == "up" {
                index.checked_sub(1)
            } else {
                index.checked_add(1).filter(|i| *i < list.len())
            };
            if let Some(destination) = adjacent
                && index < list.len()
            {
                list.swap(index, destination);
            }
        }
        _ => return Err("Unknown list operation".into()),
    }
    Ok(())
}
pub(super) fn edit_list(doc: &Document, draft: &mut Draft, operation: &str) -> Result<(), String> {
    if !native(draft.tool) {
        return Err("Open a native creature first".into());
    }
    if choose_group(draft, operation) {
        return Ok(());
    }
    let path = selected_path(draft);
    let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
    let group = if draft.tool == Tool::Feltling {
        "routes"
    } else {
        draft.native_group
    };
    let selected_row = segments
        .get(1)
        .filter(|segment| **segment == group)
        .and_then(|_| segments.get(2))
        .and_then(|s| s.parse::<usize>().ok());
    if !matches!(operation, "add") && selected_row.is_none() {
        return Err("Select a field in the route / list row first".into());
    }
    let selected = selected_row.unwrap_or(0);
    let point_index = segments
        .get(4)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let point = draft.tool == Tool::Feltling
        && (matches!(operation, "point_add" | "point_remove")
            || segments.get(3) == Some(&"points") && matches!(operation, "up" | "down"));
    let root = format!("/{}/{}", key(draft.tool), group);
    let list_path = if point {
        format!("{root}/{selected}/points")
    } else {
        root
    };
    let mut record = draft.record()?;
    let definition = record
        .get_mut(key(draft.tool))
        .and_then(Value::as_object_mut)
        .ok_or("Missing native controller")?;
    if !definition.contains_key(group) {
        drop(definition.insert(group.into(), json!([])));
    }
    let list = record
        .pointer_mut(&list_path)
        .and_then(Value::as_array_mut)
        .ok_or("Select a route before adding points")?;
    let row_index = if point { point_index } else { selected };
    mutate_list(doc, list, group, point, row_index, operation)?;
    let focus_path = if matches!(operation, "add" | "point_add") {
        format!("{list_path}/{}", list.len().saturating_sub(1))
    } else {
        list_path
    };
    refill(draft, record);
    if let Some(cursor) = draft
        .fields
        .iter()
        .position(|(f, _)| f.path.starts_with(&focus_path))
    {
        draft.native_cursor = cursor;
        draft.page = cursor / 6;
    }
    Ok(())
}
pub(super) fn draw_actions(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if draft.tool == Tool::TallGuest {
        for (x, label, op) in [
            (126.0, "Patrol", "patrol"),
            (211.0, "Prey", "prey"),
            (296.0, "Passages", "passage"),
        ] {
            ui::popup_button(
                v,
                hits,
                [x, 54.0, 80.0, 9.0],
                label,
                ui::Action::Author(Command::NativeList(op)),
            );
        }
    }
    for (index, (label, op)) in [
        ("+ Row", "add"),
        ("- Row", "remove"),
        ("+ Point", "point_add"),
        ("- Point", "point_remove"),
        ("Up", "up"),
        ("Down", "down"),
    ]
    .into_iter()
    .enumerate()
    {
        if draft.tool == Tool::TallGuest && matches!(op, "point_add" | "point_remove") {
            continue;
        }
        let x = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(55.0, 126.0);
        ui::popup_button(
            v,
            hits,
            [x, 198.0, 52.0, 11.0],
            label,
            ui::Action::Author(Command::NativeList(op)),
        );
    }
    ui::text(
        v,
        "Select a row field first; Up/Down reorder that row. Feet are world metres.",
        126.0,
        211.0,
        ui::MUTED,
        90,
    );
}

fn feltling_fields(result: &mut Vec<Field>, root: &str, definition: Option<&Value>) {
    result.push(text(
        "/feltling/mode",
        "Mode: guide / wait / talk / cheer / flee",
        "wait",
    ));
    vector(
        result,
        "/feltling/look_at",
        [
            "Look at X / blank = none",
            "Look at Y / world m",
            "Look at Z / world m",
        ],
        true,
    );
    result.extend([
        number(
            "/feltling/guide_recovery/stop_metres",
            "Stop strictly beyond metres / blank 10",
            "",
        ),
        number(
            "/feltling/guide_recovery/resume_metres",
            "Resume at/below metres / blank 6",
            "",
        ),
    ]);
    if let Some(routes) = definition
        .and_then(|v| v.get("routes"))
        .and_then(Value::as_array)
    {
        for (index, route) in routes.iter().enumerate() {
            result.push(dynamic(
                format!("{root}/routes/{index}/id"),
                "Route ID",
                Kind::Text,
                "route",
            ));
            point_fields(
                result,
                &format!("{root}/routes/{index}/points"),
                route.get("points"),
            );
        }
    }
}
fn guest_fields(result: &mut Vec<Field>, root: &str, definition: Option<&Value>) {
    point_fields(
        result,
        &format!("{root}/patrol"),
        definition.and_then(|v| v.get("patrol")),
    );
    if let Some(prey) = definition
        .and_then(|v| v.get("prey_targets"))
        .and_then(Value::as_array)
    {
        for index in 0..prey.len() {
            result.push(dynamic(
                format!("{root}/prey_targets/{index}"),
                "Prey / Enter cycles native Feltlings",
                Kind::Text,
                "",
            ));
        }
    }
    if let Some(passages) = definition
        .and_then(|v| v.get("passages"))
        .and_then(Value::as_array)
    {
        for (index, _) in passages.iter().enumerate() {
            let p = format!("{root}/passages/{index}");
            result.push(dynamic(
                format!("{p}/id"),
                "Passage ID",
                Kind::Text,
                "passage",
            ));
            vector(
                result,
                &format!("{p}/entry"),
                ["Entry feet X", "Entry feet Y", "Entry feet Z"],
                false,
            );
            vector(
                result,
                &format!("{p}/exit"),
                ["Exit feet X", "Exit feet Y", "Exit feet Z"],
                false,
            );
            result.push(dynamic(
                format!("{p}/crouch_only"),
                "Crouch only / protected required",
                Kind::Bool,
                "false",
            ));
            vector(
                result,
                &format!("{p}/protected/min"),
                [
                    "Protected min X / blank = none",
                    "Protected min Y",
                    "Protected min Z",
                ],
                true,
            );
            vector(
                result,
                &format!("{p}/protected/max"),
                ["Protected max X", "Protected max Y", "Protected max Z"],
                true,
            );
        }
    }
}

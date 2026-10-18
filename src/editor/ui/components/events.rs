//! Event navigation, conditions and actions are separate bounded subpanels.

use super::{
    Action, App, Field, Hit, INK, Kind, MUTED, ObjectId, Target, Value, Vertex, button, field_row,
    pagination, text,
};

pub(super) fn draw(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    section: usize,
    offset: usize,
) {
    let Some((doc, id)) = app.document.as_ref().zip(app.selected) else {
        return;
    };
    let binding = app.selected_binding();
    let record = doc
        .component_object(id)
        .and_then(|object| object.get("bindings"))
        .and_then(|bindings| bindings.get(binding));
    match section {
        2 => event_panel(app, v, hits, id, binding, record),
        3 => conditions(app, v, hits, record, binding, offset),
        4 => actions(app, v, hits, record),
        11 => condition_choices(app, v, hits, offset),
        _ => {}
    }
}

fn event_panel(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    id: ObjectId,
    binding: usize,
    record: Option<&Value>,
) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };

    event_controls(v, hits, binding, doc.binding_count(id), record.is_some());
    let events: &'static [&'static str] = if matches!(id, ObjectId::Trigger(_)) {
        &["enter_volume", "exit_volume"]
    } else {
        &[
            "interact",
            "spawn",
            "object_state",
            "caught",
            "ai_state",
            "timer",
            "sequence_complete",
            "animation_complete",
        ]
    };
    field_row(
        app,
        v,
        hits,
        Field::new(
            Target::Binding(binding),
            "on",
            "Event",
            Kind::Choice(events),
            "\"interact\"",
        ),
        0,
        97.0,
        240.0,
    );
    if record.is_some() {
        for (row, field) in [
            Field::new(
                Target::Binding(binding),
                "once",
                "Once per reset",
                Kind::Bool,
                "false",
            ),
            Field::new(
                Target::Binding(binding),
                "cooldown_seconds",
                "Cooldown seconds",
                Kind::Number,
                "0.0",
            ),
            Field::new(
                Target::Binding(binding),
                "key",
                "Event key filter",
                Kind::Text,
                "\"\"",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            field_row(app, v, hits, field, row, 110.0, 240.0);
        }
    } else {
        text(
            v,
            "Choose an event, then add its first action or script.",
            140.0,
            121.0,
            MUTED,
            68,
        );
    }
    button(
        v,
        hits,
        [140.0, 155.0, 117.0, 11.0],
        "Conditions >",
        Action::Components(3),
        record.is_some(),
    );
    button(
        v,
        hits,
        [263.0, 155.0, 117.0, 11.0],
        "Actions / scripts >",
        Action::Components(4),
        true,
    );
    text(
        v,
        &format!(
            "Binding {} / {}",
            binding.saturating_add(1),
            doc.binding_count(id)
        ),
        140.0,
        174.0,
        MUTED,
        60,
    );
}

fn event_controls(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    binding: usize,
    count: usize,
    exists: bool,
) {
    for (x, label, action, enabled) in [
        (200.0, "<", Action::BindingSelect(false), binding > 0),
        (
            223.0,
            ">",
            Action::BindingSelect(true),
            binding.saturating_add(1) < count,
        ),
        (246.0, "+ New", Action::BindingNew, count < 16),
        (307.0, "Remove", Action::BindingRemove, exists),
    ] {
        button(
            v,
            hits,
            [x, 83.0, if x < 240.0 { 19.0 } else { 73.0 }, 10.0],
            label,
            action,
            enabled,
        );
    }
}

fn condition_choices(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, offset: usize) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };

    let choices = doc.condition_choices();
    for (row, (index, condition)) in choices
        .iter()
        .enumerate()
        .skip(offset.saturating_mul(6))
        .take(6)
        .enumerate()
    {
        let target = condition
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let name = condition.get("name").and_then(Value::as_str).unwrap_or("?");
        button(
            v,
            hits,
            [
                140.0,
                f32::from(u16::try_from(row).unwrap_or(0)).mul_add(13.0, 97.0),
                240.0,
                11.0,
            ],
            &format!("{target} / {name}"),
            Action::ConditionChoose(index),
            true,
        );
    }
    pagination(v, hits, 11, offset, choices.len().div_ceil(6));
}

fn conditions(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    record: Option<&Value>,
    binding: usize,
    offset: usize,
) {
    let condition_list = record
        .and_then(|entry| entry.get("when"))
        .and_then(Value::as_array);
    button(
        v,
        hits,
        [263.0, 83.0, 117.0, 10.0],
        "+ State condition >",
        Action::Components(11),
        condition_list.is_none_or(|entries| entries.len() < 8),
    );
    if let Some(conditions) = condition_list {
        for (row, (index, condition)) in conditions
            .iter()
            .enumerate()
            .skip(offset.saturating_mul(3))
            .take(3)
            .enumerate()
        {
            let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(27.0, 97.0);
            let target = condition
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("?");
            let name = condition
                .get("name")
                .or_else(|| condition.get("check"))
                .and_then(Value::as_str)
                .unwrap_or("?");
            text(v, &format!("{target} / {name}"), 140.0, y, INK, 54);
            if condition.get("check").and_then(Value::as_str) == Some("state") {
                field_row(
                    app,
                    v,
                    hits,
                    Field::new(
                        Target::Condition(binding, index),
                        "equals",
                        "Equals",
                        Kind::State,
                        "false",
                    ),
                    0,
                    y + 10.0,
                    206.0,
                );
            }
            button(
                v,
                hits,
                [350.0, y + 10.0, 30.0, 11.0],
                "-",
                Action::ConditionRemove(index),
                true,
            );
        }
        pagination(v, hits, 3, offset, conditions.len().div_ceil(3));
    }
}

fn actions(_app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, record: Option<&Value>) {
    let actions = record
        .and_then(|entry| entry.get("actions"))
        .and_then(Value::as_array);
    for (row, action) in actions.into_iter().flatten().enumerate().take(8) {
        let kind = action
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("Action");
        let target = action
            .get("target")
            .or_else(|| action.get("handler"))
            .or_else(|| action.get("point"))
            .or_else(|| action.get("group"))
            .or_else(|| action.get("level"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let y = f32::from(u16::try_from(row).unwrap_or(0)).mul_add(10.0, 97.0);
        text(v, &format!("{kind} {target}"), 140.0, y + 2.0, INK, 58);
        button(
            v,
            hits,
            [350.0, y, 30.0, 9.0],
            "-",
            Action::RemoveTriggerStep(row),
            true,
        );
    }
    button(
        v,
        hits,
        [140.0, 182.0, 117.0, 10.0],
        "+ Add action >",
        Action::GameplayActions,
        actions.is_none_or(|entries| entries.len() < 8),
    );
    button(
        v,
        hits,
        [263.0, 182.0, 117.0, 10.0],
        "Script handler >",
        Action::Scripts,
        actions.is_none_or(|entries| entries.len() < 8),
    );
}

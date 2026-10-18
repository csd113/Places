//! Selection-specific capability layouts.

use super::{Action, App, C, Hit, INK, MUTED, ObjectId, S, Vertex, button, property, text};

pub(super) fn light_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    object: Option<&serde_json::Value>,
) {
    button(
        vertices,
        hits,
        [140.0, 83.0, 240.0, 10.0],
        object
            .and_then(|record| record.get("fixture"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Fixture >"),
        Action::GamePick(C::Fixture),
        true,
    );
    for (row, right, label, setting) in [
        (0, false, "Power", S::Brightness),
        (0, true, "Range m", S::Range),
        (1, false, "Red", S::Red),
        (1, true, "Green", S::Green),
        (2, false, "Blue", S::Blue),
        (2, true, "Face glow", S::Emission),
        (3, false, "Casts light", S::Enabled),
        (3, true, "Switchable", S::Switchable),
    ] {
        property(app, vertices, hits, row, right, label, setting);
    }
    text(
        vertices,
        "Mount surface aligns the fixture back to a wall/ceiling.",
        140.0,
        153.0,
        MUTED,
        68,
    );
    button(
        vertices,
        hits,
        [140.0, 165.0, 117.0, 11.0],
        "Mount surface",
        Action::MountSurface,
        true,
    );
    button(
        vertices,
        hits,
        [263.0, 165.0, 117.0, 11.0],
        if object
            .and_then(|record| record.get("mount"))
            .and_then(serde_json::Value::as_str)
            == Some("wall")
        {
            "Mount: Wall >"
        } else {
            "Mount: Ceiling >"
        },
        Action::ToggleLightMount,
        true,
    );
}

pub(super) fn audio_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    object: Option<&serde_json::Value>,
) {
    button(
        vertices,
        hits,
        [140.0, 83.0, 240.0, 10.0],
        object
            .and_then(|record| record.get("sound"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Sound >"),
        Action::GamePick(C::Sound),
        true,
    );
    for (row, right, label, setting) in [
        (0, false, "Gain", S::Gain),
        (0, true, "Positional", S::Spatial),
        (1, false, "Loop", S::Loop),
        (1, true, "Play on load", S::Playing),
        (2, false, "Enabled", S::Enabled),
    ] {
        property(app, vertices, hits, row, right, label, setting);
    }
    text(
        vertices,
        "Positional off = ambient throughout the level.",
        140.0,
        143.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Play on load off lets triggers start the cue.",
        140.0,
        154.0,
        MUTED,
        68,
    );
}

pub(super) fn spawn_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    _object: Option<&serde_json::Value>,
    i: usize,
) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };

    let point = doc.level().spawn_points.get(i);
    let template = point.and_then(|p| {
        doc.level()
            .spawn_templates
            .iter()
            .find(|record| record.id == p.template)
    });
    button(
        vertices,
        hits,
        [140.0, 83.0, 240.0, 10.0],
        template.map_or("Choose model >", |record| record.model.as_str()),
        Action::GamePick(C::EntityModel),
        true,
    );
    property(app, vertices, hits, 0, false, "Lifetime s", S::Lifetime);
    button(
        vertices,
        hits,
        [263.0, 95.0, 117.0, 11.0],
        &format!(
            "Behavior: {} >",
            behavior_name(template.map(|definition| definition.components.as_slice()))
        ),
        Action::GamePick(C::EntityBehavior),
        true,
    );
    button(
        vertices,
        hits,
        [140.0, 109.0, 240.0, 11.0],
        &format!(
            "Template: {} >",
            point.map_or("?", |definition| definition.template.as_str())
        ),
        Action::GamePick(C::SpawnTemplate),
        true,
    );
    text(
        vertices,
        "Move this marker to choose where entities appear.",
        140.0,
        126.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Trigger Actions > Spawn entity targets this point.",
        140.0,
        138.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "One active spawn per point; templates can be shared.",
        140.0,
        150.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Lifetime 0 keeps the entity until despawn/reset.",
        140.0,
        162.0,
        MUTED,
        68,
    );
}

pub(super) fn door_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    _object: Option<&serde_json::Value>,
) {
    text(
        vertices,
        "Closed on load / E interacts / hinge coordinates",
        140.0,
        84.0,
        MUTED,
        68,
    );
    for (row, right, label, setting) in [
        (0, false, "Two-way", S::Bidirectional),
        (0, true, "Locked", S::Locked),
        (1, false, "Swing deg", S::Swing),
        (1, true, "Speed deg/s", S::Speed),
    ] {
        property(app, vertices, hits, row, right, label, setting);
    }
    text(
        vertices,
        "Leaf width/height/thickness are in Transform.",
        140.0,
        132.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Place its hinge in an existing wall opening.",
        140.0,
        144.0,
        MUTED,
        68,
    );
}

pub(super) fn trigger_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    object: Option<&serde_json::Value>,
    page: usize,
) {
    button(
        vertices,
        hits,
        [140.0, 83.0, 117.0, 10.0],
        "Properties",
        Action::GameplayPage(0),
        true,
    );
    button(
        vertices,
        hits,
        [263.0, 83.0, 117.0, 10.0],
        "Actions >",
        Action::GameplayPage(1),
        true,
    );
    if page == 0 {
        let filter_label = object.and_then(|record| record.get("filter")).map_or_else(
            || "Player".into(),
            |record| {
                let kind = record
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("player");
                let instance = record
                    .get("instance")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                format!("Filter: {kind} {instance}")
            },
        );
        button(
            vertices,
            hits,
            [140.0, 97.0, 240.0, 11.0],
            &filter_label,
            Action::GamePick(C::Filter),
            true,
        );
        for (row, right, label, setting) in [
            (2, false, "Once", S::Once),
            (2, true, "Cooldown s", S::Cooldown),
            (3, false, "Occupancy", S::Aggregate),
            (3, true, "Event", S::ExitEvent),
        ] {
            property(app, vertices, hits, row, right, label, setting);
        }
        text(
            vertices,
            "Resize the box with Width / Height / Depth.",
            140.0,
            155.0,
            MUTED,
            68,
        );
        text(
            vertices,
            "Translucent guides appear only in this editor.",
            140.0,
            166.0,
            MUTED,
            68,
        );
    } else {
        trigger_actions(vertices, hits, object);
    }
}

fn trigger_actions(
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    object: Option<&serde_json::Value>,
) {
    let actions = object
        .and_then(|record| record.get("bindings"))
        .and_then(|record| record.get(0))
        .and_then(|record| record.get("actions"))
        .and_then(serde_json::Value::as_array);
    if let Some(steps) = actions {
        for (i, action) in steps.iter().take(8).enumerate() {
            let kind = action
                .get("action")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Action");
            let target = action
                .get("target")
                .or_else(|| action.get("point"))
                .or_else(|| action.get("group"))
                .or_else(|| action.get("script"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let label = if kind == "run_script" {
                format!(
                    "Luau {} / {target}",
                    action
                        .get("handler")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("?")
                )
            } else {
                format!("{kind} {target}")
            };
            let y = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(10.0, 97.0);
            text(vertices, &label, 140.0, y + 3.0, INK, 58);
            button(
                vertices,
                hits,
                [353.0, y, 27.0, 9.0],
                "Remove",
                Action::RemoveTriggerStep(i),
                true,
            );
        }
    }
    button(
        vertices,
        hits,
        [140.0, 180.0, 117.0, 10.0],
        "+ Add action >",
        Action::GameplayActions,
        actions.is_none_or(|record| record.len() < 8),
    );
    button(
        vertices,
        hits,
        [263.0, 180.0, 117.0, 10.0],
        "Script handler >",
        Action::Scripts,
        true,
    );
}

pub(super) fn prop_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    _object: Option<&serde_json::Value>,
    index: usize,
) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };

    let prop = doc.level().props.get(index);
    button(
        vertices,
        hits,
        [263.0, 158.0, 117.0, 11.0],
        &format!(
            "Behavior: {} >",
            behavior_name(prop.map(|definition| definition.components.as_slice()))
        ),
        Action::GamePick(C::EntityBehavior),
        true,
    );
    button(
        vertices,
        hits,
        [140.0, 83.0, 240.0, 10.0],
        prop.map_or("Entity model >", |record| record.model.as_str()),
        Action::Assets,
        true,
    );
    for (row, right, label, setting) in [
        (0, false, "Physics", S::Physics),
        (0, true, "Solid", S::Solid),
        (1, false, "Pickup", S::Pickup),
        (1, true, "Mass kg", S::Mass),
    ] {
        property(app, vertices, hits, row, right, label, setting);
    }
    text(
        vertices,
        "Physics starts frozen until touched or triggered.",
        140.0,
        129.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Yaw, pitch and roll allow intentional intersections.",
        140.0,
        140.0,
        MUTED,
        68,
    );
    button(
        vertices,
        hits,
        [140.0, 158.0, 117.0, 11.0],
        "Mount surface",
        Action::MountSurface,
        true,
    );
}

pub(super) fn fog_sheet(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    _object: Option<&serde_json::Value>,
    id: ObjectId,
) {
    for (row, right, label, setting) in [
        (0, false, "Density / m", S::FogDensity),
        (0, true, "Edge fade m", S::FogFalloff),
        (1, false, "Red", S::Red),
        (1, true, "Green", S::Green),
        (2, false, "Blue", S::Blue),
    ] {
        property(app, vertices, hits, row, right, label, setting);
    }
    text(
        vertices,
        if matches!(id, ObjectId::GroundFog(_)) {
            "Ground layer fades upward from its base."
        } else {
            "Regional fog fills the authored bounding box."
        },
        140.0,
        144.0,
        MUTED,
        68,
    );
    text(
        vertices,
        "Use Lighting preview to inspect the atmosphere.",
        140.0,
        157.0,
        MUTED,
        68,
    );
}

fn behavior_name(components: Option<&[crate::engine::level::ComponentDef]>) -> &str {
    components
        .and_then(|entries| {
            entries.iter().find_map(|component| {
                if let crate::engine::level::ComponentDef::Ai(ai) = component {
                    Some(ai.behavior.name())
                } else {
                    None
                }
            })
        })
        .unwrap_or("decorative")
}

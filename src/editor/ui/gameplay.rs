//! Compact creation and contextual capability sheets keep the viewport central.

mod sheets;
use sheets::{
    audio_sheet, door_sheet, fog_sheet, light_sheet, prop_sheet, spawn_sheet, trigger_sheet,
};

use super::{ACCENT, Action, Hit, INK, MUTED, button, text};
use crate::editor::App;
use crate::editor::document::{GameplaySetting as S, ObjectId, ObjectKind};
use crate::editor::gameplay_actions::GameplayChoice as C;
use crate::engine::render::Vertex;

pub(in crate::editor) fn create_popup(vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(vertices, "ADD TO SCENE", 140.0, 73.0, ACCENT, 60);
    text(
        vertices,
        "Built-ins / edit properties after placement",
        140.0,
        83.0,
        MUTED,
        65,
    );
    for (index, (label, kind)) in [
        ("Light fixture", ObjectKind::Light),
        ("Ambient / positional sound", ObjectKind::Audio),
        ("Entity", ObjectKind::Entity),
        ("Door (closed)", ObjectKind::Door),
        ("Trigger volume", ObjectKind::Trigger),
        ("Regional box fog", ObjectKind::FogRegion),
        ("Ground fog", ObjectKind::GroundFog),
        ("Entity spawn point", ObjectKind::SpawnPoint),
    ]
    .into_iter()
    .enumerate()
    {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(12.0, 96.0);
        button(
            vertices,
            hits,
            [140.0, y, 240.0, 10.0],
            label,
            Action::Add(kind),
            true,
        );
    }
    button(
        vertices,
        hits,
        [330.0, 190.0, 50.0, 12.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

fn property(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    row: u16,
    right: bool,
    label: &str,
    setting: S,
) {
    let Some((doc, id)) = app.document.as_ref().zip(app.selected) else {
        return;
    };
    let value = doc.gameplay_value(id, setting);
    let display = if setting == S::Physics {
        if matches!(id,ObjectId::Prop(i) if doc.level().props.get(i).is_some_and(|record|record.physics.is_some()))
        {
            "On".into()
        } else {
            "Off".into()
        }
    } else if let Some(flag) = value.as_bool() {
        if flag { "On".into() } else { "Off".into() }
    } else if let Some(number) = value.as_f64() {
        format!("{number:.2}")
    } else {
        value.as_str().unwrap_or("-").into()
    };
    let x = if right { 263.0 } else { 140.0 };
    let y = f32::from(row).mul_add(13.0, 95.0);
    button(
        vertices,
        hits,
        [x, y, 117.0, 11.0],
        &format!("{label}: {display}"),
        Action::GameSetting(setting),
        !matches!(setting, S::Pickup | S::Mass)
            || matches!(id,ObjectId::Prop(index) if doc.level().props.get(index).is_some_and(|prop|prop.physics.is_some())),
    );
}

pub(in crate::editor) fn gameplay_popup(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    page: usize,
) {
    let Some((doc, id)) = app.document.as_ref().zip(app.selected) else {
        text(vertices, "Select an object", 140.0, 80.0, INK, 50);
        return;
    };
    let (collection, object_index) = id.collection();
    let object = doc
        .raw()
        .get(collection)
        .and_then(|record| record.get(object_index));
    let identity = object
        .and_then(|record| record.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Placed object");
    let heading = match id {
        ObjectId::Light(_) => "LIGHT",
        ObjectId::Audio(_) => "SOUND",
        ObjectId::Door(_) => "DOOR",
        ObjectId::Trigger(_) => "TRIGGER",
        ObjectId::SpawnPoint(_) => "SPAWN POINT",
        ObjectId::FogRegion(_) => "REGIONAL FOG",
        ObjectId::GroundFog(_) => "GROUND FOG",
        ObjectId::Prop(_) => "PROP / ENTITY",
        ObjectId::Room(_) | ObjectId::Wall(_) | ObjectId::Ramp(_) | ObjectId::FloorRegion(_) => {
            "GEOMETRY"
        }
    };
    text(
        vertices,
        &format!("{heading} / {identity}"),
        140.0,
        73.0,
        ACCENT,
        66,
    );
    match id {
        ObjectId::Light(_) => light_sheet(app, vertices, hits, object),
        ObjectId::Audio(_) => audio_sheet(app, vertices, hits, object),
        ObjectId::SpawnPoint(i) => spawn_sheet(app, vertices, hits, object, i),
        ObjectId::Door(_) => door_sheet(app, vertices, hits, object),
        ObjectId::Trigger(_) => trigger_sheet(app, vertices, hits, object, page),
        ObjectId::Prop(index) => prop_sheet(app, vertices, hits, object, index),
        ObjectId::FogRegion(_) | ObjectId::GroundFog(_) => {
            fog_sheet(app, vertices, hits, object, id);
        }
        ObjectId::Room(_) | ObjectId::Wall(_) | ObjectId::Ramp(_) | ObjectId::FloorRegion(_) => {
            text(
                vertices,
                "Transform and Shape controls are in the inspector.",
                140.0,
                100.0,
                MUTED,
                68,
            );
        }
    }
    button(
        vertices,
        hits,
        [140.0, 196.0, 117.0, 11.0],
        "Components >",
        Action::Components(0),
        true,
    );
    button(
        vertices,
        hits,
        [330.0, 196.0, 50.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

pub(in crate::editor) fn action_popup(vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(vertices, "ADD EVENT ACTION", 140.0, 73.0, ACCENT, 60);
    for (i, (label, choice)) in [
        ("Play a sound emitter", C::SoundAction),
        ("Open a door", C::DoorAction),
        ("Spawn an entity at point", C::SpawnAction),
        ("Activate a physics group", C::PhysicsAction),
        ("Change project level", C::TransitionAction),
    ]
    .into_iter()
    .enumerate()
    {
        let y = f32::from(u16::try_from(i).unwrap_or(0)).mul_add(18.0, 94.0);
        button(
            vertices,
            hits,
            [140.0, y, 240.0, 13.0],
            label,
            Action::GamePick(choice),
            true,
        );
    }
    button(
        vertices,
        hits,
        [330.0, 190.0, 50.0, 12.0],
        "Back",
        Action::Components(4),
        true,
    );
}

pub(in crate::editor) fn choices_popup(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    choice: C,
    page: usize,
) {
    if matches!(choice, C::EntityModel | C::Fixture) {
        super::library::draw_grid(
            app,
            vertices,
            hits,
            &app.gameplay_choices(choice),
            page,
            Some(choice),
        );
        return;
    }

    text(
        vertices,
        match choice {
            C::Filter => "TRIGGER / eligible occupants",
            C::Fixture => "BUILT-IN LIGHT FIXTURES",
            C::Sound => "BUILT-IN SOUND CUES",
            C::EntityModel => "BUILT-IN ENTITY MODELS",
            C::EntityBehavior => "ENTITY / shared runtime behavior",
            C::CharacterAppearance => "MANNEQUIN / shared model appearance",
            C::SpawnTemplate => "SPAWN POINT / reusable template",
            C::SoundAction => "PLAY SOUND / choose an emitter",
            C::DoorAction => "OPEN DOOR / choose a leaf",
            C::SpawnAction => "SPAWN ENTITY / choose a point",
            C::PhysicsAction => "ACTIVATE PHYSICS / choose a group",
            C::TransitionAction => "TRANSITION / choose a project level",
        },
        140.0,
        73.0,
        ACCENT,
        68,
    );
    let choices = app.gameplay_choices(choice);
    if choices.is_empty() {
        text(
            vertices,
            "Create a matching object in the scene first.",
            140.0,
            100.0,
            MUTED,
            68,
        );
    }
    for (index, label) in choices
        .iter()
        .enumerate()
        .skip(page.saturating_mul(10))
        .take(10)
    {
        let y = f32::from(u16::try_from(index % 10).unwrap_or(0)).mul_add(10.0, 85.0);
        button(
            vertices,
            hits,
            [
                140.0,
                y,
                if choice == C::Sound { 180.0 } else { 240.0 },
                9.0,
            ],
            label,
            Action::GameChoose(choice, index),
            true,
        );
        if choice == C::Sound {
            button(
                vertices,
                hits,
                [325.0, y, 55.0, 9.0],
                "Listen",
                Action::PreviewSound(index),
                true,
            );
        }
    }
    if choice == C::Sound {
        button(
            vertices,
            hits,
            [255.0, 196.0, 65.0, 11.0],
            "Stop preview",
            Action::StopPreviewSound,
            true,
        );
    }
    for (x, label, action) in [
        (140.0, "< Prev", Action::Page(false)),
        (196.0, "Next >", Action::Page(true)),
        (330.0, "Back", Action::Gameplay),
    ] {
        button(vertices, hits, [x, 196.0, 50.0, 11.0], label, action, true);
    }
}

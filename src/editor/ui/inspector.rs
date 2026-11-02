//! Contextual right dock: common edits first, precise and specialized work disclosed.

use super::{ACCENT, Action, Hit, INK, MUTED, button, text};
use crate::editor::App;
use crate::editor::document::{GameplaySetting as Setting, ObjectId, Transform};
use crate::editor::gameplay_actions::GameplayChoice;
use crate::engine::render::Vertex;

const LEFT: f32 = 379.0;
const WIDTH: f32 = 96.0;
const ROW: f32 = 11.5;

pub(in crate::editor) fn inspector(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(vertices, "PROPERTIES", LEFT, 41.0, ACCENT, 24);
    button(
        vertices,
        hits,
        [443.0, 38.0, 32.0, 10.0],
        "Frame",
        Action::Focus,
        true,
    );
    let Some((document, id)) = app.document.as_ref().zip(app.selected) else {
        text(vertices, "Select an object", LEFT, 69.0, INK, 24);
        text(vertices, "Click in the scene or", LEFT, 84.0, MUTED, 24);
        text(vertices, "choose one in Scene.", LEFT, 94.0, MUTED, 24);
        return;
    };
    let object_name = document
        .objects()
        .iter()
        .find(|object| object.id == id)
        .map_or("Placed object", |object| object.label.as_str());
    text(vertices, object_name, LEFT, 54.0, INK, 24);
    text(
        vertices,
        if app.editable(id) {
            heading(id)
        } else {
            "Locked / hidden"
        },
        LEFT,
        64.0,
        MUTED,
        24,
    );
    let Ok(pose) = document.transform(id) else {
        text(vertices, "Selection unavailable", LEFT, 82.0, MUTED, 24);
        return;
    };
    let mut top = 77.0;
    if matches!(id, ObjectId::Light(_)) {
        light_essentials(app, vertices, hits, &mut top);
    } else {
        for (index, label, value) in transform_fields(Some(id), pose) {
            if supported(id, index) && essential(id, index) {
                scalar(app, vertices, hits, index, top, label, value);
                top += ROW;
            }
        }
        object_essentials(app, vertices, hits, id, &mut top);
    }
    row_button(
        vertices,
        hits,
        &mut top,
        "Exact transform >",
        Action::Transform,
        app.editable(id),
    );
    top += 2.0;
    row_button(
        vertices,
        hits,
        &mut top,
        if app.dock.advanced {
            "[-] Advanced"
        } else {
            "[+] Advanced"
        },
        Action::Advanced,
        true,
    );
    advanced_controls(app, vertices, hits, id, pose, &mut top);
    row_button(
        vertices,
        hits,
        &mut top,
        "All tools >",
        Action::Workspace,
        true,
    );
}

fn advanced_controls(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    id: ObjectId,
    pose: Transform,
    top: &mut f32,
) {
    if app.dock.advanced {
        for (index, label, value) in transform_fields(Some(id), pose) {
            if supported(id, index) && !essential(id, index) {
                scalar(app, vertices, hits, index, *top, label, value);
                *top += ROW;
            }
        }
        let (label, action) = if matches!(id, ObjectId::Light(_)) {
            (
                "Flicker / glow >",
                Action::Flicker(crate::editor::flicker::Command::Open),
            )
        } else {
            match id {
                ObjectId::Column(_) => (
                    "Pillar details >",
                    Action::Author(crate::editor::authoring::Command::Open(
                        crate::editor::authoring::Tool::Column,
                    )),
                ),
                ObjectId::Pillar(_) => (
                    "Pillar details >",
                    Action::Author(crate::editor::authoring::Command::Open(
                        crate::editor::authoring::Tool::Pillar,
                    )),
                ),
                ObjectId::ArcWall(_) => (
                    "Curve details >",
                    Action::Author(crate::editor::authoring::Command::Open(
                        crate::editor::authoring::Tool::ArcWall,
                    )),
                ),
                ObjectId::Decal(_)
                | ObjectId::Room(_)
                | ObjectId::Wall(_)
                | ObjectId::Prop(_)
                | ObjectId::Ramp(_)
                | ObjectId::FloorRegion(_)
                | ObjectId::Light(_)
                | ObjectId::Audio(_)
                | ObjectId::Door(_)
                | ObjectId::Trigger(_)
                | ObjectId::FogRegion(_)
                | ObjectId::GroundFog(_)
                | ObjectId::SpawnPoint(_) => ("Object settings >", Action::Gameplay),
            }
        };
        row_button(vertices, hits, top, label, action, app.editable(id));
    }
}

fn object_essentials(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    id: ObjectId,
    top: &mut f32,
) {
    let editable = app.editable(id);
    if matches!(
        id,
        ObjectId::Room(_)
            | ObjectId::Wall(_)
            | ObjectId::Door(_)
            | ObjectId::Column(_)
            | ObjectId::Pillar(_)
            | ObjectId::ArcWall(_)
            | ObjectId::Ramp(_)
            | ObjectId::FloorRegion(_)
    ) {
        let material = object_value(app, "material").unwrap_or("Default");
        let name = app
            .catalog
            .get(material)
            .map_or(material, |entry| entry.display_name.as_str());
        row_button(
            vertices,
            hits,
            top,
            &format!("Material: {name} >"),
            Action::Materials,
            editable,
        );
    }
    match id {
        ObjectId::Room(_) => {
            row_button(vertices, hits, top, "Room shape >", Action::Shape, editable);
        }
        ObjectId::Wall(_) => row_button(
            vertices,
            hits,
            top,
            "Make doorway",
            Action::Place(crate::editor::construction::Tool::Doorway),
            editable,
        ),
        ObjectId::Door(_) => {
            row_button(
                vertices,
                hits,
                top,
                "Door style >",
                Action::DoorAssets,
                editable,
            );
            property(app, vertices, hits, *top, "Swing deg", Setting::Swing);
            *top += ROW;
        }
        ObjectId::Prop(index) => {
            let model = app
                .document
                .as_ref()
                .and_then(|document| document.level().props.get(index))
                .map_or("Model", |prop| prop.model.as_str());
            row_button(
                vertices,
                hits,
                top,
                &format!("Model: {model} >"),
                Action::Assets,
                editable,
            );
            row_button(
                vertices,
                hits,
                top,
                "Mount to surface",
                Action::MountSurface,
                editable,
            );
        }
        ObjectId::Decal(_)
        | ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Ramp(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => {}
    }
}

fn light_essentials(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>, top: &mut f32) {
    let editable = app.selected.is_some_and(|id| app.editable(id));
    let fixture = object_value(app, "fixture").unwrap_or("Fixture");
    let name = app
        .catalog
        .get(fixture)
        .map_or(fixture, |entry| entry.display_name.as_str());
    row_button(
        vertices,
        hits,
        top,
        &format!("Fixture: {name} >"),
        Action::GamePick(GameplayChoice::Fixture),
        editable,
    );
    for (label, setting) in [("Power", Setting::Brightness), ("Range m", Setting::Range)] {
        property(app, vertices, hits, *top, label, setting);
        *top += ROW;
    }
    for (x, label, setting) in [
        (LEFT, "R", Setting::Red),
        (411.0, "G", Setting::Green),
        (443.0, "B", Setting::Blue),
    ] {
        button(
            vertices,
            hits,
            [x, *top, 31.0, 10.0],
            &format!("{label} {}", setting_display(app, setting)),
            Action::GameSetting(setting),
            editable,
        );
    }
    *top += ROW;
    row_button(
        vertices,
        hits,
        top,
        if object_value(app, "mount") == Some("wall") {
            "Mount: Wall >"
        } else {
            "Mount: Ceiling >"
        },
        Action::ToggleLightMount,
        editable,
    );
    row_button(
        vertices,
        hits,
        top,
        "Align to surface",
        Action::MountSurface,
        editable,
    );
}

fn object_value<'a>(app: &'a App, key: &str) -> Option<&'a str> {
    let (document, id) = app.document.as_ref().zip(app.selected)?;
    let (collection, index) = id.collection();
    document
        .raw()
        .get(collection)?
        .get(index)?
        .get(key)?
        .as_str()
}

fn setting_display(app: &App, setting: Setting) -> String {
    let Some((document, id)) = app.document.as_ref().zip(app.selected) else {
        return "-".into();
    };
    let value = document.gameplay_value(id, setting);
    value.as_f64().map_or_else(
        || {
            value.as_bool().map_or_else(
                || value.as_str().unwrap_or("-").into(),
                |flag| if flag { "On" } else { "Off" }.into(),
            )
        },
        |number| format!("{number:.2}"),
    )
}

fn property(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    top: f32,
    label: &str,
    setting: Setting,
) {
    text(vertices, label, LEFT, top + 2.0, MUTED, 11);
    button(
        vertices,
        hits,
        [422.0, top, 53.0, 10.0],
        &setting_display(app, setting),
        Action::GameSetting(setting),
        app.selected.is_some_and(|id| app.editable(id)),
    );
}

fn row_button(
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    top: &mut f32,
    label: &str,
    action: Action,
    enabled: bool,
) {
    button(
        vertices,
        hits,
        [LEFT, *top, WIDTH, 10.0],
        label,
        action,
        enabled,
    );
    *top += ROW;
}

const fn essential(id: ObjectId, index: usize) -> bool {
    match id {
        ObjectId::Light(_) => false,
        ObjectId::Prop(_) => matches!(index, 0..=3 | 6),
        ObjectId::Audio(_) | ObjectId::SpawnPoint(_) => index <= 2,
        ObjectId::Decal(_) | ObjectId::Ramp(_) => matches!(index, 3..=6),
        ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Room(_)
        | ObjectId::Wall(_)
        | ObjectId::FloorRegion(_)
        | ObjectId::Door(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_) => matches!(index, 3..=5),
    }
}

const fn supported(id: ObjectId, index: usize) -> bool {
    !((matches!(id, ObjectId::Pillar(_)) && matches!(index, 5 | 6))
        || (matches!(id, ObjectId::Prop(_)) && matches!(index, 4 | 5))
        || (matches!(id, ObjectId::FloorRegion(_)) && matches!(index, 4 | 6))
        || (matches!(id, ObjectId::Decal(_)) && index == 5)
        || (!matches!(id, ObjectId::Prop(_)) && matches!(index, 11 | 12))
        || (matches!(
            id,
            ObjectId::Light(_) | ObjectId::Audio(_) | ObjectId::SpawnPoint(_)
        ) && matches!(index, 3..=5))
        || (matches!(id, ObjectId::Audio(_)) && index == 6)
        || (matches!(
            id,
            ObjectId::Trigger(_) | ObjectId::FogRegion(_) | ObjectId::GroundFog(_)
        ) && index == 6))
}

const fn transform_fields(
    id: Option<ObjectId>,
    pose: Transform,
) -> [(usize, &'static str, f32); 9] {
    let ramp = matches!(id, Some(ObjectId::Ramp(_)));
    let region = matches!(id, Some(ObjectId::FloorRegion(_)));
    let prop = matches!(id, Some(ObjectId::Prop(_)));
    let circle = matches!(id, Some(ObjectId::Pillar(_)));
    let [pos_x, pos_y, pos_z] = pose.position;
    let [width, height, depth] = pose.size;
    [
        (0, "X", pos_x),
        (
            1,
            if ramp || region {
                "Offset"
            } else if prop {
                "Y local"
            } else {
                "Y"
            },
            pos_y,
        ),
        (2, "Z", pos_z),
        (
            3,
            if prop {
                "Scale"
            } else if circle {
                "Diameter"
            } else {
                "Width"
            },
            width,
        ),
        (4, if ramp { "Rise" } else { "Height" }, height),
        (
            5,
            if matches!(id, Some(ObjectId::Door(_))) {
                "Thick"
            } else {
                "Depth"
            },
            depth,
        ),
        (
            6,
            if ramp {
                "Turn"
            } else if matches!(id, Some(ObjectId::Decal(_))) {
                "Spin"
            } else {
                "Yaw"
            },
            pose.yaw_degrees,
        ),
        (11, "Pitch", pose.pitch_degrees),
        (12, "Roll", pose.roll_degrees),
    ]
}

const fn heading(id: ObjectId) -> &'static str {
    match id {
        ObjectId::Room(_) => "Room",
        ObjectId::Wall(_) => "Wall",
        ObjectId::Prop(_) => "Prop / entity",
        ObjectId::Door(_) => "Door / hinge",
        ObjectId::Light(_) => "Light / mount plane",
        ObjectId::Audio(_) => "Sound / position",
        ObjectId::SpawnPoint(_) => "Spawn / position",
        ObjectId::Column(_) => "Rectangular pillar",
        ObjectId::Pillar(_) => "Circular pillar / centre",
        ObjectId::ArcWall(_) => "Curved wall / centre",
        ObjectId::Ramp(_) => "Ramp / offset and rise",
        ObjectId::FloorRegion(_) => "Region / surface Y",
        ObjectId::Decal(_) => "Sign / centre / spin",
        ObjectId::Trigger(_) => "Trigger / bounds",
        ObjectId::FogRegion(_) => "Fog / bounds",
        ObjectId::GroundFog(_) => "Ground fog / base",
    }
}

fn scalar(
    app: &App,
    vertices: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    index: usize,
    top: f32,
    label: &str,
    value: f32,
) {
    let editable = app.selected.is_some_and(|id| app.editable(id));
    let value_label = if matches!(index, 6 | 11 | 12) {
        format!("{label} deg")
    } else if matches!(app.selected, Some(ObjectId::Prop(_))) && index == 3 {
        label.to_owned()
    } else if label == "Diameter" {
        "Diam m".into()
    } else {
        format!("{label} m")
    };
    text(vertices, &value_label, LEFT, top + 2.0, MUTED, 11);
    button(
        vertices,
        hits,
        [416.0, top, 35.0, 10.0],
        &format!("{value:.3}"),
        Action::Field(index),
        editable,
    );
    for (x, mark, positive) in [(453.0, "-", false), (465.0, "+", true)] {
        button(
            vertices,
            hits,
            [x, top, 10.0, 10.0],
            mark,
            Action::Nudge(index, positive),
            editable,
        );
    }
}

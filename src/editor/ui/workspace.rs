//! Progressive disclosure for tools beyond the common room-building flow.
use super::{ACCENT, Action, Hit, INK, MUTED, PANEL, button, rect, text};
use crate::editor::{App, document::ObjectId};
use crate::engine::render::Vertex;

pub(in crate::editor) fn draw(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    rect(v, [130.0, 65.0, 260.0, 177.0], PANEL);
    text(v, "CUSTOMIZE YOUR SPACE", 140.0, 74.0, ACCENT, 62);
    text(v, "Deeper authoring tools", 140.0, 85.0, MUTED, 68);
    button(
        v,
        hits,
        [264.0, 84.0, 116.0, 11.0],
        if app.view.simple {
            "Precision controls >"
        } else {
            "< Simple form controls"
        },
        Action::Mode,
        true,
    );
    text(v, "BUILD AND APPEARANCE", 140.0, 99.0, INK, 32);
    text(v, "BEHAVIOR AND PROJECT", 264.0, 99.0, INK, 32);
    appearance(app, v, hits);
    project_tools(app, v, hits);
    text(
        v,
        "Test: unbaked gameplay. Final build: production lighting.",
        140.0,
        231.0,
        MUTED,
        68,
    );
    button(
        v,
        hits,
        [330.0, 70.0, 50.0, 11.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

fn appearance(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    for (x, y, label, action, enabled) in [
        (140.0, 110.0, "Geometry / stairs >", Action::Geometry, true),
        (
            140.0,
            127.0,
            "Props / sounds / signs >",
            Action::Create,
            true,
        ),
        (
            140.0,
            144.0,
            "Surface material >",
            Action::Materials,
            matches!(
                app.selected,
                Some(
                    ObjectId::Column(_)
                        | ObjectId::Pillar(_)
                        | ObjectId::ArcWall(_)
                        | ObjectId::Room(_)
                        | ObjectId::Wall(_)
                        | ObjectId::Ramp(_)
                        | ObjectId::FloorRegion(_)
                        | ObjectId::Door(_)
                )
            ),
        ),
        (
            140.0,
            161.0,
            "Surface grime / reuse >",
            Action::Reuse(crate::editor::reuse::Command::Open),
            true,
        ),
        (
            140.0,
            178.0,
            "Light circuits >",
            Action::Lighting(crate::editor::lighting::Command::Open),
            true,
        ),
        (
            140.0,
            195.0,
            "Local lighting preview",
            Action::Lighting(crate::editor::lighting::Command::Local),
            matches!(
                app.selected,
                Some(ObjectId::Room(_) | ObjectId::FloorRegion(_))
            ) && !app.viewport.is_preparing(),
        ),
        (
            140.0,
            212.0,
            "Environment / space >",
            Action::Author(crate::editor::authoring::Command::Hub),
            true,
        ),
    ] {
        button(v, hits, [x, y, 116.0, 14.0], label, action, enabled);
    }
}

fn project_tools(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let selected = app.selected.is_some();
    let (context, context_action, context_enabled) = selection_tool(app);
    for (x, y, label, action, enabled) in [
        (
            264.0,
            110.0,
            "Object settings >",
            Action::Gameplay,
            selected,
        ),
        (264.0, 127.0, context, context_action, context_enabled),
        (264.0, 144.0, "Scripts / events >", Action::Scripts, true),
        (
            264.0,
            161.0,
            "Navigation inspection >",
            Action::Navigation,
            true,
        ),
        (
            264.0,
            178.0,
            "Layers / focus / lock >",
            Action::Layers,
            true,
        ),
        (264.0, 195.0, "Safety check >", Action::Safety, true),
        (
            264.0,
            212.0,
            "Project / export >",
            Action::Project,
            app.compile.is_none() && app.play.is_none() && !app.dialog_open,
        ),
    ] {
        button(v, hits, [x, y, 116.0, 14.0], label, action, enabled);
    }
}

fn selection_tool(app: &App) -> (&'static str, Action, bool) {
    let selected = app.selected.is_some();
    match app.selected {
        Some(ObjectId::Light(_)) => (
            "Light flicker >",
            Action::Flicker(crate::editor::flicker::Command::Open),
            true,
        ),
        Some(ObjectId::Prop(index))
            if app
                .document
                .as_ref()
                .and_then(|d| d.level().props.get(index))
                .is_some_and(|p| crate::editor::ducts::supported(&p.model)) =>
        {
            (
                "Duct joins >",
                Action::Duct(crate::editor::ducts::Command::Open),
                true,
            )
        }
        Some(ObjectId::Prop(_)) => (
            "NPC behavior >",
            Action::Behavior(crate::editor::behavior::Command::Open),
            true,
        ),
        Some(ObjectId::Decal(_)) => (
            "Wall sign / decal >",
            Action::Sign(crate::editor::signs::Command::Open),
            true,
        ),
        Some(ObjectId::Wall(_)) => (
            "Make doorway",
            Action::Place(crate::editor::construction::Tool::Doorway),
            true,
        ),
        Some(ObjectId::Room(_)) => ("Room shape >", Action::Shape, true),
        Some(
            ObjectId::Column(_)
            | ObjectId::Pillar(_)
            | ObjectId::ArcWall(_)
            | ObjectId::Ramp(_)
            | ObjectId::FloorRegion(_)
            | ObjectId::Audio(_)
            | ObjectId::SpawnPoint(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_),
        )
        | None => (
            "Actions and conditions >",
            Action::GameplayActions,
            selected,
        ),
    }
}

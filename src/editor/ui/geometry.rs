//! Creation choices and contextual floor operations stay out of the viewport.

use super::{ACCENT, Action, Hit, INK, MUTED, button, text};
use crate::editor::App;
use crate::editor::document::{ObjectId, ObjectKind};
use crate::engine::render::Vertex;

pub(in crate::editor) fn geometry_popup(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(v, "ADD GEOMETRY", 140.0, 74.0, ACCENT, 40);
    for (x, y, label, kind) in [
        (140.0, 88.0, "New Room", ObjectKind::Room),
        (264.0, 88.0, "Wall", ObjectKind::Wall),
        (140.0, 109.0, "Ramp", ObjectKind::Ramp),
        (264.0, 109.0, "Global recess", ObjectKind::FloorRegion),
    ] {
        button(v, hits, [x, y, 116.0, 16.0], label, Action::Add(kind), true);
    }
    button(
        v,
        hits,
        [140.0, 130.0, 116.0, 13.0],
        "Space / environment >",
        Action::Author(crate::editor::authoring::Command::Hub),
        true,
    );
    for (x, label, tool) in [
        (
            140.0,
            "Rectangular pillar",
            crate::editor::authoring::Tool::Column,
        ),
        (
            264.0,
            "Circular pillar",
            crate::editor::authoring::Tool::Pillar,
        ),
    ] {
        button(
            v,
            hits,
            [x, 146.0, 116.0, 11.0],
            label,
            Action::Author(crate::editor::authoring::Command::Create(tool)),
            true,
        );
    }
    button(
        v,
        hits,
        [264.0, 130.0, 116.0, 13.0],
        "Connect two floors >",
        Action::StairWizard,
        true,
    );
    let floor = matches!(app.selected, Some(ObjectId::Room(_)));
    button(
        v,
        hits,
        [140.0, 157.0, 240.0, 15.0],
        "Room shape tools >",
        Action::Shape,
        floor,
    );
    button(
        v,
        hits,
        [140.0, 177.0, 240.0, 12.0],
        "Automatic trim for wall selection >",
        Action::Author(crate::editor::authoring::Command::Open(
            crate::editor::authoring::Tool::WallTrim,
        )),
        matches!(app.selected, Some(ObjectId::Wall(_))),
    );
    button(
        v,
        hits,
        [330.0, 190.0, 50.0, 12.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

pub(in crate::editor) fn shape_popup(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    text(v, "ROOM SHAPE", 140.0, 74.0, ACCENT, 45);
    for (index, label) in ["West", "East", "North", "South"].into_iter().enumerate() {
        let x = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(61.0, 140.0);
        button(
            v,
            hits,
            [x, 151.0, 57.0, 11.0],
            label,
            Action::Field(index.saturating_add(20)),
            true,
        );
    }
    text(
        v,
        "OPENING RECTANGLE / world metres",
        140.0,
        86.0,
        MUTED,
        68,
    );
    let Some([x, z, width, depth]) = app.opening_rect else {
        text(v, "Select a room to edit its shape.", 140.0, 101.0, INK, 68);
        button(
            v,
            hits,
            [330.0, 190.0, 50.0, 12.0],
            "Close",
            Action::ClosePopup,
            true,
        );
        return;
    };
    for (left, top, label, field, value) in [
        (140.0, 97.0, "X", 7, x),
        (264.0, 97.0, "Z", 8, z),
        (140.0, 110.0, "Width", 9, width),
        (264.0, 110.0, "Depth", 10, depth),
    ] {
        text(v, label, left, top + 3.0, MUTED, 7);
        button(
            v,
            hits,
            [left + 26.0, top, 90.0, 10.0],
            &format!("{value:.3}"),
            Action::Field(field),
            true,
        );
    }
    text(
        v,
        "Cut removes base+ceiling; four strips remain.",
        140.0,
        125.0,
        INK,
        68,
    );
    button(
        v,
        hits,
        [140.0, 136.0, 240.0, 15.0],
        "Cut opening / one undo step",
        Action::CutOpening,
        true,
    );
    button(
        v,
        hits,
        [140.0, 165.0, 116.0, 15.0],
        "Add upper floor",
        Action::UpperFloor,
        true,
    );
    text(v, "Copies +room height.", 264.0, 167.0, INK, 33);
    text(v, "Lower ceiling stays.", 264.0, 176.0, MUTED, 33);
    text(v, "Cut both storeys", 140.0, 191.0, MUTED, 50);
    text(v, "for a through shaft.", 140.0, 200.0, MUTED, 50);
    button(
        v,
        hits,
        [330.0, 190.0, 50.0, 12.0],
        "Close",
        Action::ClosePopup,
        true,
    );
}

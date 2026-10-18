//! A deliberate advanced pose sheet leaves ordinary single-field edits intact.

use super::{ACCENT, Action, Hit, INK, MUTED, popup_button, rect, text};
use crate::editor::document::ObjectId;
use crate::editor::transform::Draft;
use crate::engine::render::Vertex;

pub(in crate::editor) fn draw(draft: &Draft, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    if draft.preview {
        draw_preview(v, hits);
        return;
    }
    rect(v, [0.0, 0.0, 480.0, 272.0], [0.0, 0.0, 0.0, 0.6]);
    rect(v, [116.0, 49.0, 354.0, 184.0], [0.075, 0.11, 0.14, 1.0]);
    text(v, "COMPLETE TRANSFORM", 126.0, 58.0, ACCENT, 75);
    text(
        v,
        "Edit all fields, preview, then apply once.",
        126.0,
        70.0,
        MUTED,
        80,
    );
    for (index, label) in [
        "X", "Y", "Z", "Width", "Height", "Depth", "Yaw", "Pitch", "Roll",
    ]
    .into_iter()
    .enumerate()
    {
        if matches!(draft.id, ObjectId::Prop(_)) && matches!(index, 4 | 5) {
            continue;
        }
        if !matches!(draft.id, ObjectId::Prop(_)) && index >= 7 {
            continue;
        }
        let col = u16::try_from(index / 3).unwrap_or(0);
        let row = u16::try_from(index % 3).unwrap_or(0);
        let x = f32::from(col).mul_add(111.0, 126.0);
        let y = f32::from(row).mul_add(30.0, 87.0);
        let field_label = if index == 3 && matches!(draft.id, ObjectId::Prop(_)) {
            "Scale"
        } else if index == 4 && matches!(draft.id, ObjectId::Ramp(_)) {
            "Rise"
        } else {
            label
        };
        text(v, field_label, x, y, MUTED, 23);
        let value = draft.fields.get(index).map_or("", String::as_str);
        popup_button(
            v,
            hits,
            [x, y + 9.0, 101.0, 13.0],
            value,
            Action::TransformField(index),
        );
        if draft.active == Some(index) {
            text(v, "<", x + 94.0, y + 12.0, ACCENT, 1);
        }
    }
    text(
        v,
        "Y uses the selection's authored coordinate basis.",
        126.0,
        184.0,
        INK,
        85,
    );
    text(
        v,
        "Cmd/Ctrl A clears a field. Enter finishes typing.",
        126.0,
        195.0,
        MUTED,
        85,
    );
    for (x, label, action) in [
        (126.0, "Preview", Action::TransformPreview),
        (238.0, "Apply pose", Action::TransformApply),
        (350.0, "Cancel", Action::ClosePopup),
    ] {
        popup_button(v, hits, [x, 213.0, 101.0, 13.0], label, action);
    }
}

fn draw_preview(v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    rect(v, [122.0, 207.0, 348.0, 31.0], [0.075, 0.11, 0.14, 1.0]);
    text(
        v,
        "POSE PREVIEW / source unchanged",
        130.0,
        211.0,
        ACCENT,
        60,
    );
    popup_button(
        v,
        hits,
        [130.0, 222.0, 105.0, 12.0],
        "Edit pose",
        Action::TransformField(0),
    );
    popup_button(
        v,
        hits,
        [242.0, 222.0, 105.0, 12.0],
        "Apply pose",
        Action::TransformApply,
    );
    popup_button(
        v,
        hits,
        [354.0, 222.0, 105.0, 12.0],
        "Cancel",
        Action::ClosePopup,
    );
}

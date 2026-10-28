//! Selection-specific transforms keep the central viewport visible.

use super::{ACCENT, Action, Hit, MUTED, button, text};
use crate::editor::App;
use crate::editor::document::ObjectId;
use crate::engine::level::WallAxis;
use crate::engine::render::Vertex;

pub(in crate::editor) fn inspector(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let region = matches!(app.selected, Some(ObjectId::FloorRegion(_)));
    let circle = matches!(app.selected, Some(ObjectId::Pillar(_)));
    let prop = matches!(app.selected, Some(ObjectId::Prop(_)));
    let marker = matches!(
        app.selected,
        Some(ObjectId::Light(_) | ObjectId::Audio(_) | ObjectId::SpawnPoint(_))
    );
    let box_only = matches!(
        app.selected,
        Some(ObjectId::Trigger(_) | ObjectId::FogRegion(_) | ObjectId::GroundFog(_))
    );
    text(vertices, heading(app), 5.0, 131.0, ACCENT, 15);
    if app.selected.is_some() {
        let room = matches!(app.selected, Some(ObjectId::Room(_)));
        button(
            vertices,
            hits,
            [
                if room { 64.0 } else { 67.0 },
                128.0,
                if room { 43.0 } else { 40.0 },
                10.0,
            ],
            "Pose >",
            Action::Transform,
            true,
        );
    }
    if let Some(transform) = app
        .selected
        .and_then(|id| app.document.as_ref()?.transform(id).ok())
    {
        let fields = transform_fields(app.selected, transform);
        let mut row = 0_u16;
        for (index, label, value) in fields {
            if (circle && matches!(index, 5 | 6))
                || (prop && matches!(index, 4 | 5))
                || (region && matches!(index, 4 | 6))
                || (!prop && matches!(index, 11 | 12))
                || (marker && (3..=5).contains(&index))
                || (matches!(app.selected, Some(ObjectId::Audio(_))) && index == 6)
                || (box_only && index == 6)
            {
                continue;
            }
            let top = f32::from(row).mul_add(9.0, 141.0);
            row = row.saturating_add(1);
            scalar(vertices, hits, index, top, label, value);
        }
        if region {
            text(vertices, "Y relative to room base", 5.0, 197.0, MUTED, 28);
        }
        if marker {
            text(
                vertices,
                "Configure > for properties",
                5.0,
                194.0,
                MUTED,
                29,
            );
        }
    } else {
        text(vertices, "Select an object", 5.0, 145.0, MUTED, 27);
    }
}

const fn transform_fields(
    id: Option<ObjectId>,
    pose: crate::editor::document::Transform,
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
        (6, if ramp { "Turn" } else { "Yaw" }, pose.yaw_degrees),
        (11, "Pitch", pose.pitch_degrees),
        (12, "Roll", pose.roll_degrees),
    ]
}

fn heading(app: &App) -> &'static str {
    match app.selected {
        Some(ObjectId::Ramp(index)) => app
            .document
            .as_ref()
            .and_then(|d| d.level().ramps.get(index))
            .map_or("RAMP", |r| match (r.axis(), r.rise > 0.0) {
                (WallAxis::X, true) => "RAMP / UP +X",
                (WallAxis::X, false) => "RAMP / UP -X",
                (WallAxis::Z, true) => "RAMP / UP +Z",
                (WallAxis::Z, false) => "RAMP / UP -Z",
            }),
        Some(ObjectId::Column(_)) => "RECTANGULAR PILLAR",
        Some(ObjectId::Pillar(_)) => "CIRCULAR PILLAR / CENTRE",
        Some(ObjectId::ArcWall(_)) => "CURVED WALL / CENTRE",
        Some(ObjectId::FloorRegion(_)) => "REGION / SURFACE Y",
        Some(ObjectId::Light(_)) => "LIGHT / MOUNT PLANE",
        Some(ObjectId::Audio(_)) => "SOUND / POSITION",
        Some(ObjectId::SpawnPoint(_)) => "SPAWN / POSITION",
        Some(ObjectId::Door(_)) => "DOOR / HINGE",
        Some(ObjectId::Trigger(_)) => "TRIGGER / BOUNDS",
        Some(ObjectId::FogRegion(_)) => "FOG / BOUNDS",
        Some(ObjectId::GroundFog(_)) => "GROUND FOG / BASE",
        Some(ObjectId::Room(_) | ObjectId::Wall(_) | ObjectId::Prop(_)) | None => "TRANSFORM",
    }
}
fn scalar(
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    index: usize,
    top: f32,
    label: &str,
    value: f32,
) {
    text(
        v,
        label,
        5.0,
        top + 3.0,
        super::AXIS_COLORS.get(index).copied().unwrap_or(MUTED),
        8,
    );
    button(
        v,
        hits,
        [33.0, top, 52.0, 8.0],
        &format!("{value:.3}"),
        Action::Field(index),
        true,
    );
    for (x, mark, positive) in [(87.0, "-", false), (98.0, "+", true)] {
        button(
            v,
            hits,
            [x, top, 9.0, 8.0],
            mark,
            Action::Nudge(index, positive),
            true,
        );
    }
}

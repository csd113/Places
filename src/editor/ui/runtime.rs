//! In-editor test chrome and paged authored messages share the native renderer.
use super::{ACCENT, Action, Hit, INK, LINE, MUTED, PANEL, button, rect, text};
use crate::editor::{App, test_session::MessageView};
use crate::engine::render::Vertex;

pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) -> usize {
    text(
        vertices,
        "PLACES / TEST IN EDITOR / UNBAKED",
        7.0,
        7.0,
        ACCENT,
        86,
    );
    button(
        vertices,
        hits,
        [371.0, 5.0, 103.0, 13.0],
        if app.runtime_preparing() {
            "Cancel test / Esc"
        } else {
            "Stop test / Esc"
        },
        if app.runtime_preparing() {
            Action::Cancel
        } else {
            Action::Stop
        },
        true,
    );
    text(
        vertices,
        app.runtime_warning()
            .unwrap_or("Preparing an isolated test / no final lighting bake"),
        7.0,
        21.0,
        [0.97, 0.80, 0.48, 1.0],
        132,
    );
    text(
        vertices,
        app.runtime_status()
            .unwrap_or("Preparing isolated simulation..."),
        7.0,
        247.0,
        INK,
        128,
    );
    text(
        vertices,
        "WASD move | E use/pickup/drop | F light | F1 hint | Space jump | Ctrl crouch | Esc return",
        7.0,
        258.0,
        MUTED,
        132,
    );
    let modal_start = vertices.len();
    let message = app.runtime_message();
    if app.runtime_active() && message.is_none() {
        crosshair(vertices);
    }
    if let Some(view) = message {
        message_card(vertices, &view);
    }
    modal_start
}

fn crosshair(vertices: &mut Vec<Vertex>) {
    // This suffix uses the centered modal transform at every window aspect.
    rect(vertices, [238.5, 135.75, 3.0, 0.5], INK);
    rect(vertices, [239.75, 134.5, 0.5, 3.0], INK);
}

fn message_card(vertices: &mut Vec<Vertex>, view: &MessageView<'_>) {
    // Only this appended geometry receives the centered modal transform.
    // The ordinary test chrome and Stop target keep their window anchors.
    rect(vertices, [0.0, 38.0, 480.0, 204.0], [0.0, 0.0, 0.0, 0.45]);
    rect(vertices, [112.0, 62.0, 256.0, 164.0], PANEL);
    text(vertices, view.title, 126.0, 75.0, ACCENT, 68);
    for (index, row) in view
        .rows
        .iter()
        .skip(view.page.saturating_mul(6))
        .take(6)
        .enumerate()
    {
        let top = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(14.0, 93.0);
        // Rows are already wrapped by the runtime at 54 characters. The text
        // budget deliberately exceeds that width, retaining every authored row.
        text(vertices, row, 126.0, top, INK, 68);
    }
    rect(vertices, [126.0, 181.0, 228.0, 0.4], LINE);
    let pages = view.rows.len().div_ceil(6).max(1);
    text(
        vertices,
        &format!(
            "Page {} / {pages}   PgUp / PgDn",
            view.page.saturating_add(1)
        ),
        126.0,
        192.0,
        MUTED,
        68,
    );
    text(
        vertices,
        if view.completed {
            "Test complete / Esc returns to editing"
        } else {
            "Enter continues / Esc returns to editing"
        },
        126.0,
        207.0,
        INK,
        68,
    );
}

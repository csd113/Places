//! In-editor test chrome and paged authored messages share the native renderer.
use super::{ACCENT, Action, Hit, INK, LINE, MUTED, PANEL, button, rect, text};
use crate::editor::{App, test_session::MessageView};
use crate::engine::render::Vertex;

pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>) -> usize {
    health_overlay(app, vertices);
    if let Some(prompt) = app.viewport.runtime_encounter_prompt() {
        text(vertices, &prompt, 150.0, 205.0, INK, 100);
    }
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
            "Stop test"
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
    let footer_start = vertices.len();
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
        if app
            .viewport
            .runtime_health()
            .is_some_and(|(health, settings)| settings.enabled && health.is_dead())
        {
            "Health depleted | Esc or Stop test returns to editing"
        } else if app.viewport.runtime_cinematic() {
            "Cinematic camera | Player controls suspended | Stop test returns to editing"
        } else {
            "WASD move | E use/pickup/drop | F light | F1 hint | Space jump | Ctrl crouch | Stop test returns"
        },
        7.0,
        258.0,
        MUTED,
        132,
    );
    super::dock::anchor_bottom(
        app,
        vertices.get_mut(footer_start..).unwrap_or_default(),
        &mut [],
    );
    let modal_start = vertices.len();
    let message = app.runtime_message();
    if app.runtime_active()
        && message.is_none()
        && !app.viewport.runtime_cinematic()
        && !app
            .viewport
            .runtime_health()
            .is_some_and(|(health, settings)| settings.enabled && health.is_dead())
    {
        crosshair(vertices);
    }
    if let Some(view) = message {
        message_card(vertices, &view);
    } else if let Some((speaker, rows)) = app.viewport.runtime_subtitles() {
        subtitle_card(vertices, speaker, rows);
    }
    modal_start
}

fn subtitle_card(vertices: &mut Vec<Vertex>, speaker: &str, rows: &[String]) {
    // The queue/layout contract is shared; glyph UVs belong to the editor atlas.
    let width = |value: &str| crate::game::ui::text_width(value, 0.5);
    let content_width = rows
        .iter()
        .map(|row| width(row))
        .fold(width(speaker), f32::max);
    let panel_width = (content_width + 20.0).max(112.0);
    let height = if rows.len() > 1 { 37.0 } else { 27.0 };
    let top = 244.0 - height;
    rect(
        vertices,
        [(480.0 - panel_width) * 0.5, top, panel_width, height],
        [0.012, 0.018, 0.023, 0.72],
    );
    text(
        vertices,
        speaker,
        (480.0 - width(speaker)) * 0.5,
        top + 5.0,
        [0.93, 0.83, 0.58, 1.0],
        128,
    );
    for (index, row) in rows.iter().enumerate() {
        let y = f32::from(u16::try_from(index).unwrap_or(0)).mul_add(10.0, top + 15.0);
        text(
            vertices,
            row,
            (480.0 - width(row)) * 0.5,
            y,
            [1.0, 0.97, 0.90, 1.0],
            128,
        );
    }
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

/// Share heart geometry, but use the editor atlas and footer-safe anchoring.
fn health_overlay(app: &App, vertices: &mut Vec<Vertex>) {
    let Some((health, settings)) = app.viewport.runtime_health() else {
        return;
    };
    if !settings.enabled || !settings.show_player_health {
        return;
    }
    let y_offset = app.viewport.layout().bottom_offset() - 28.0;
    if health.max_hp() > 40 {
        text(
            vertices,
            &format!("HP {} / {}", health.current_hp(), health.max_hp()),
            12.0,
            242.0 + y_offset,
            INK,
            50,
        );
        return;
    }
    let hearts = vertices.len();
    crate::game::ui::append_player_health(vertices, &health, &settings);
    let source = crate::engine::font::health_heart_uv();
    let destination = crate::engine::font::editor_health_heart_uv();
    for vertex in vertices.get_mut(hearts..).unwrap_or_default() {
        vertex.pos[1] += y_offset;
        vertex.uv[0] = ((vertex.uv[0] - source[0]) / (source[2] - source[0]))
            .mul_add(destination[2] - destination[0], destination[0]);
        vertex.uv[1] = ((vertex.uv[1] - source[1]) / (source[3] - source[1]))
            .mul_add(destination[3] - destination[1], destination[1]);
    }
}

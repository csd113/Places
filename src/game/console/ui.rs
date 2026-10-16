//! Compact console in the existing 480x272 text/quad pass.
use super::{COLUMNS, Console, VISIBLE_LINES};
use crate::engine::render::Vertex;
use crate::game::ui::{add_rect_rgba, draw_text};

pub(super) fn geometry(console: &Console, noclip: bool) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    add_rect_rgba(
        &mut vertices,
        10.0,
        10.0,
        470.0,
        220.0,
        [0.025, 0.035, 0.045, 0.97],
    );
    draw_text(
        &mut vertices,
        if noclip {
            "DEVELOPER CONSOLE  |  NOCLIP ON"
        } else {
            "DEVELOPER CONSOLE  |  NOCLIP OFF"
        },
        18.0,
        18.0,
        0.8,
        [0.75, 0.88, 1.0],
    );
    draw_text(
        &mut vertices,
        "` / Esc close | Up/Down history | PgUp/PgDn/wheel scroll",
        18.0,
        33.0,
        0.8,
        [0.65, 0.70, 0.76],
    );
    let lines = console.wrapped_lines();
    let end = lines.len().saturating_sub(console.scroll);
    let start = end.saturating_sub(VISIBLE_LINES);
    let mut y = 49.0;
    for line in lines.iter().take(end).skip(start) {
        draw_text(&mut vertices, line, 18.0, y, 0.8, [0.90, 0.92, 0.94]);
        y += 9.0;
    }
    add_rect_rgba(
        &mut vertices,
        16.0,
        188.0,
        464.0,
        205.0,
        [0.10, 0.14, 0.18, 1.0],
    );
    let tail: String = console
        .input
        .chars()
        .skip(
            console
                .input
                .len()
                .saturating_sub(COLUMNS.saturating_sub(3)),
        )
        .collect();
    draw_text(
        &mut vertices,
        &format!("> {tail}_"),
        18.0,
        193.0,
        0.8,
        [1.0, 0.88, 0.62],
    );
    draw_text(
        &mut vertices,
        if console.scroll == 0 {
            "World paused while typing. /help for commands."
        } else {
            "Viewing older output. PgDn returns to latest."
        },
        18.0,
        211.0,
        0.7,
        [0.65, 0.70, 0.76],
    );
    vertices
}

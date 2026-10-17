//! Built-in assets reuse one paged, focused selection surface.

use super::{Action, Hit, INK, popup_button, text};
use crate::editor::App;
use crate::engine::render::Vertex;

pub(in crate::editor) fn draw_library(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    materials: bool,
    page: usize,
) {
    text(
        v,
        if materials {
            "BUILT-IN MATERIALS / apply to selection"
        } else {
            "BUILT-IN PROPS / select then + Prop"
        },
        140.0,
        73.0,
        INK,
        65,
    );
    for (index, label) in app
        .library(materials)
        .into_iter()
        .enumerate()
        .skip(page.saturating_mul(10))
        .take(10)
    {
        let offset = index % 10;
        let y = f32::from(u16::try_from(offset).unwrap_or(0)).mul_add(10.0, 85.0);
        popup_button(
            v,
            hits,
            [140.0, y, 240.0, 9.0],
            &label,
            Action::ChooseAsset(index),
        );
    }
    for (x, label, action) in [
        (140.0, "< Prev", Action::Page(false)),
        (196.0, "Next >", Action::Page(true)),
        (330.0, "Close", Action::ClosePopup),
    ] {
        popup_button(v, hits, [x, 190.0, 50.0, 12.0], label, action);
    }
}

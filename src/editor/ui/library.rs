//! Theme-filtered actual model/material previews with keyboard and paging.
use super::{Action, Hit, INK, MUTED, popup_button, rect, text};
use crate::editor::{
    App,
    browser::{PAGE_SIZE, choice_action, tile_bounds},
    gameplay_actions::GameplayChoice,
};
use crate::engine::render::Vertex;

pub(in crate::editor) fn draw_library(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    materials: bool,
    page: usize,
) {
    draw_grid(app, v, hits, &app.library(materials), page, None);
}

pub(in crate::editor) fn draw_grid(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    ids: &[String],
    page: usize,
    choice: Option<GameplayChoice>,
) {
    rect(v, [116.0, 43.0, 354.0, 197.0], [0.075, 0.11, 0.14, 1.0]);
    text(
        v,
        &format!("Search: {}_", app.browser.query),
        123.0,
        48.0,
        INK,
        82,
    );
    if matches!(
        app.popup,
        Some(crate::editor::actions::Popup::Library {
            materials: true,
            page: _
        })
    ) && matches!(
        app.selected,
        Some(crate::editor::document::ObjectId::Room(_))
    ) {
        popup_button(
            v,
            hits,
            [398.0, 48.0, 62.0, 10.0],
            if app.browser.material_ceiling {
                "Ceiling"
            } else {
                "Floor"
            },
            Action::MaterialTarget,
        );
    }
    let doors = matches!(app.popup, Some(crate::editor::actions::Popup::DoorAssets));
    let themes = app.browser_themes();
    if doors {
        text(v, "Shared / built-in door styles", 123.0, 63.0, INK, 70);
    } else {
        popup_button(
            v,
            hits,
            [123.0, 60.0, 240.0, 12.0],
            themes
                .get(app.browser.theme)
                .map_or("All themes", String::as_str),
            Action::BrowserTheme,
        );
    }
    text(
        v,
        &format!(
            "{} / {}",
            page.saturating_add(1),
            (ids.len().saturating_sub(1) / PAGE_SIZE).saturating_add(1)
        ),
        378.0,
        63.0,
        MUTED,
        20,
    );
    for (index, id) in ids
        .iter()
        .enumerate()
        .skip(page.saturating_mul(PAGE_SIZE))
        .take(PAGE_SIZE)
    {
        let [x, y, width, height] = tile_bounds(index % PAGE_SIZE);
        popup_button(
            v,
            hits,
            [x, y, width, height + 10.0],
            "",
            choice_action(choice, index),
        );
        text(v, app.browser.feedback(id), x + 2.0, y + 25.0, MUTED, 19);
        let name = app.catalog.get(id).map_or_else(
            || crate::editor::browser::door_name(id),
            |e| e.display_name.as_str(),
        );
        text(v, name, x, y + height + 1.0, INK, 20);
        if app.ui_focus == Some(choice_action(choice, index)) {
            text(v, id, 123.0, 223.0, INK, 82);
        }
    }
    if ids.is_empty() {
        text(
            v,
            "No matches. Backspace clears the search.",
            130.0,
            110.0,
            INK,
            75,
        );
    }
    for (x, label, action) in [
        (123.0, "< Prev", Action::Page(false)),
        (184.0, "Next >", Action::Page(true)),
        (408.0, "Close", Action::ClosePopup),
    ] {
        popup_button(v, hits, [x, 231.0, 54.0, 9.0], label, action);
    }
}

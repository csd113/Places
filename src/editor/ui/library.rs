//! Theme-filtered actual model/material previews with keyboard and paging.
use super::{Action, Hit, INK, MUTED, popup_button, rect, text};
use crate::editor::{
    App,
    browser::{choice_action, tile_bounds},
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
    let page_size = app.browser_page_size();
    rect(v, [116.0, 43.0, 354.0, 197.0], [0.075, 0.11, 0.14, 1.0]);
    text(
        v,
        &crate::game::ui::fit_text(&format!("Search: {}_", app.browser.query), 170.0, 0.5),
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
            [300.0, 48.0, 62.0, 10.0],
            if app.browser.material_ceiling {
                "Ceiling"
            } else {
                "Surface"
            },
            Action::MaterialTarget,
        );
    }
    popup_button(
        v,
        hits,
        [370.0, 48.0, 92.0, 10.0],
        if app.browser.compact {
            "Tiles: Small"
        } else {
            "Tiles: Medium"
        },
        Action::ThumbnailSize,
    );
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
            (ids.len()
                .saturating_sub(1)
                .checked_div(page_size)
                .unwrap_or(0))
            .saturating_add(1)
        ),
        378.0,
        63.0,
        MUTED,
        20,
    );
    draw_tiles(app, v, hits, ids, page, choice);
    if ids.is_empty() {
        text(
            v,
            "No matches. Cmd/Ctrl A clears search.",
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

fn draw_tiles(
    app: &App,
    v: &mut Vec<Vertex>,
    hits: &mut Vec<Hit>,
    ids: &[String],
    page: usize,
    choice: Option<GameplayChoice>,
) {
    let page_size = app.browser_page_size();
    for (index, id) in ids
        .iter()
        .enumerate()
        .skip(page.saturating_mul(page_size))
        .take(page_size)
    {
        let [x, y, width, height] = tile_bounds(
            index.checked_rem(page_size).unwrap_or(0),
            app.browser.compact,
        );
        popup_button(
            v,
            hits,
            [
                x,
                y,
                if app.browser.compact { 54.0 } else { 64.0 },
                height + 8.0,
            ],
            "",
            choice_action(choice, index),
        );
        if app.browser.image(id).is_none() {
            text(v, app.browser.feedback(id), x + 2.0, y + 10.0, MUTED, 8);
        }
        let assigned = app.selected.and_then(|selected| {
            let doc = app.document.as_ref()?;
            let (key, i) = selected.collection();
            doc.raw()
                .get(key)?
                .get(i)?
                .get(if app.browser.material_ceiling {
                    "ceiling_material"
                } else {
                    "material"
                })?
                .as_str()
        }) == Some(id.as_str());
        if assigned {
            for edge in [
                [x - 1.0, y - 1.0, width + 2.0, 1.0],
                [x - 1.0, y + height, width + 2.0, 1.0],
                [x - 1.0, y, 1.0, height],
                [x + width, y, 1.0, height],
            ] {
                rect(v, edge, [0.37, 0.86, 0.71, 1.0]);
            }
        }
        let name = app.catalog.get(id).map_or_else(
            || crate::editor::browser::door_name(id),
            |e| e.display_name.as_str(),
        );
        text(
            v,
            name,
            x,
            y + height + 1.0,
            if assigned {
                [0.37, 0.86, 0.71, 1.0]
            } else {
                INK
            },
            if app.browser.compact { 13 } else { 16 },
        );
        if app.ui_focus == Some(choice_action(choice, index)) {
            text(v, &format!("{name} / {id}"), 123.0, 223.0, INK, 82);
        }
    }
}

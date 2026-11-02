//! Theme-filtered actual model/material previews with keyboard and paging.
use super::{
    Action, Hit, INK, MUTED, button,
    dock::{Catalog, Tab},
    popup_button, rect, text,
};
use crate::editor::{
    App,
    browser::{DOCK_PAGE_SIZE, choice_action, dock_tile_bounds, tile_bounds},
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
    rect(v, [116.0, 43.0, 354.0, 197.0], super::PANEL);
    popup_button(
        v,
        hits,
        [123.0, 46.0, 170.0, 12.0],
        &search_label(app, "Search assets..."),
        Action::BrowserSearch,
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
        if app.focused_action() == Some(choice_action(choice, index)) {
            text(v, &format!("{name} / {id}"), 123.0, 223.0, INK, 82);
        }
    }
}

/// Inline catalog tiles occupy the left dock; the modal catalog remains available
/// for contextual selectors. Both use the same bounded committed-image cache.
pub(in crate::editor) fn draw_dock(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>) {
    let top = match app.dock.tab {
        Tab::Build => 132.0,
        Tab::Assets => 74.0,
        Tab::Lighting => 98.0,
    };
    let placeholder = match app.dock.catalog {
        Catalog::Materials => "Search materials...",
        Catalog::Models => "Search props...",
        Catalog::Fixtures => "Search fixtures...",
        Catalog::Doors => "Search doors...",
    };
    button(
        v,
        hits,
        [5.0, top, 90.0, 11.0],
        &search_label(app, placeholder),
        Action::BrowserSearch,
        true,
    );
    if app.dock.catalog == Catalog::Doors {
        text(v, "Built-in door styles", 7.0, top + 16.0, MUTED, 23);
    } else {
        button(
            v,
            hits,
            [
                5.0,
                top + 13.0,
                if app.dock.tab == Tab::Build {
                    59.0
                } else {
                    90.0
                },
                10.0,
            ],
            app.browser_themes()
                .get(app.browser.theme)
                .map_or("All themes", String::as_str),
            Action::BrowserTheme,
            true,
        );
    }
    let ids = app.dock_choices();
    draw_dock_tiles(app, v, hits, &ids);
    if ids.is_empty() {
        text(v, "No matching assets", 7.0, top + 36.0, INK, 25);
        text(v, "Clear search to browse", 7.0, top + 46.0, MUTED, 25);
    }
    draw_dock_pages(app, v, hits, &ids, top);
}

fn draw_dock_pages(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, ids: &[String], top: f32) {
    let page_size = DOCK_PAGE_SIZE;
    let final_page = ids
        .len()
        .saturating_sub(1)
        .checked_div(page_size)
        .unwrap_or(0);
    if app.dock.tab == Tab::Build {
        button(
            v,
            hits,
            [66.0, top + 13.0, 13.0, 10.0],
            "<",
            Action::Page(false),
            app.dock.page > 0,
        );
        button(
            v,
            hits,
            [82.0, top + 13.0, 13.0, 10.0],
            ">",
            Action::Page(true),
            app.dock.page < final_page,
        );
        return;
    }
    button(
        v,
        hits,
        [5.0, 220.0, 26.0, 10.0],
        "Prev",
        Action::Page(false),
        app.dock.page > 0,
    );
    text(
        v,
        &format!(
            "{} / {}",
            app.dock.page.saturating_add(1),
            final_page.saturating_add(1)
        ),
        35.0,
        222.0,
        MUTED,
        13,
    );
    button(
        v,
        hits,
        [65.0, 220.0, 30.0, 10.0],
        "Next >",
        Action::Page(true),
        app.dock.page < final_page,
    );
}

fn search_label(app: &App, placeholder: &str) -> String {
    if app.dock.search && app.focused_action() == Some(Action::BrowserSearch) {
        format!("{}|", app.browser.query)
    } else if app.browser.query.is_empty() {
        placeholder.into()
    } else {
        app.browser.query.clone()
    }
}

fn draw_dock_tiles(app: &App, v: &mut Vec<Vertex>, hits: &mut Vec<Hit>, ids: &[String]) {
    let page_size = DOCK_PAGE_SIZE;
    for (offset, (index, id)) in ids
        .iter()
        .enumerate()
        .skip(app.dock.page.saturating_mul(page_size))
        .take(page_size)
        .enumerate()
    {
        let [x, y, width, height] = dock_tile_bounds(offset, app.dock.tab);
        button(
            v,
            hits,
            [x, y, width, height + 7.0],
            "",
            Action::DockChoose(index),
            app.document.is_some(),
        );
        if app.browser.image(id).is_none() {
            text(
                v,
                if app.browser.feedback(id) == "Preview unavailable" {
                    "No image"
                } else {
                    "Loading..."
                },
                x + 2.0,
                y + 12.0,
                MUTED,
                10,
            );
        }
        let name = app.catalog.get(id).map_or_else(
            || crate::editor::browser::door_name(id),
            |entry| entry.display_name.as_str(),
        );
        let selected = dock_assigned(app, id);
        if selected {
            for edge in [
                [x - 0.6, y - 0.6, width + 1.2, 0.6],
                [x - 0.6, y + height, width + 1.2, 0.6],
                [x - 0.6, y, 0.6, height],
                [x + width, y, 0.6, height],
            ] {
                rect(v, edge, super::ACCENT);
            }
        }
        text(
            v,
            &format!("{}{name}", if selected { "* " } else { "" }),
            x,
            y + height + 1.0,
            if selected { super::ACCENT } else { INK },
            11,
        );
    }
}

fn dock_assigned(app: &App, id: &str) -> bool {
    let Some(selected) = app.selected else {
        return false;
    };
    let Some(doc) = app.document.as_ref() else {
        return false;
    };
    let (collection, index) = selected.collection();
    let Some(object) = doc
        .raw()
        .get(collection)
        .and_then(|objects| objects.get(index))
    else {
        return false;
    };
    let key = match app.dock.catalog {
        Catalog::Materials => {
            if app.browser.material_ceiling {
                "ceiling_material"
            } else {
                "material"
            }
        }
        Catalog::Models => "model",
        Catalog::Fixtures => "fixture",
        Catalog::Doors => "kind",
    };
    let assigned = object.get(key).and_then(serde_json::Value::as_str);
    if app.dock.catalog == Catalog::Doors {
        Some(assigned.unwrap_or("interior")) == id.strip_prefix("editor:door_")
    } else {
        assigned == Some(id)
    }
}

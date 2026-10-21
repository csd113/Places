//! Committed catalog artwork, decoded one visible tile per frame, never baked
//! or rendered continuously. Twenty-four 160x120 tiles cost at most 1.76 MiB.
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{App, actions::Popup, gameplay_actions::GameplayChoice, ui::Action};
use crate::engine::{assets::AssetEntry, materials::RawImage, render::UiImage};

pub(super) const DOOR_PREVIEWS: [&str; 3] = [
    "editor:door_interior",
    "editor:door_sauna",
    "editor:door_restroom",
];

pub(super) const PAGE_SIZE: usize = 8;

#[derive(Default)]
pub(super) struct Browser {
    pub query: String,
    pub material_ceiling: bool,
    pub theme: usize,
    themes: Vec<String>,
    index: Option<serde_json::Value>,
    tiles: BTreeMap<String, Result<Arc<RawImage>, String>>,
}

impl Browser {
    pub fn new(catalog: &crate::engine::assets::AssetCatalog) -> Self {
        let mut themes = vec!["All themes".into()];
        let unique = catalog
            .entries()
            .into_iter()
            .map(|e| {
                e.theme
                    .as_ref()
                    .map_or("Shared", |t| t.as_str())
                    .to_string()
            })
            .collect::<std::collections::BTreeSet<_>>();
        themes.extend(unique);
        Self {
            themes,
            ..Self::default()
        }
    }

    pub fn cached_tiles(&self) -> usize {
        self.tiles.len()
    }

    pub fn image(&self, id: &str) -> Option<&Arc<RawImage>> {
        self.tiles.get(id)?.as_ref().ok()
    }

    pub fn feedback(&self, id: &str) -> &str {
        match self.tiles.get(id) {
            Some(Ok(_)) => "",
            Some(Err(_)) => "Preview unavailable",
            None => "Loading preview...",
        }
    }

    fn prepare_one(&mut self, visible: &[String]) {
        if self.tiles.len() >= 24 {
            self.tiles.retain(|id, _| visible.contains(id));
        }
        let Some(id) = visible.iter().find(|id| !self.tiles.contains_key(*id)) else {
            return;
        };
        let loaded = crate::engine::paths::assets().and_then(|root| {
            if self.index.is_none() {
                let bytes = crate::engine::project::read_bounded_file(
                    &root.join("editor/previews.json"),
                    1_048_576,
                )?;
                self.index = Some(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?);
            }
            let path = self
                .index
                .as_ref()
                .and_then(|v| v.get(id))
                .and_then(|v| v.get("path"))
                .and_then(serde_json::Value::as_str)
                .ok_or("No committed preview for this asset")?;
            let image = crate::engine::materials::load_png_relative(root, path)?;
            if image.width != 160 || image.height != 120 {
                return Err("Invalid catalog preview dimensions".into());
            }
            Ok(Arc::new(image))
        });
        let _previous = self.tiles.insert(id.clone(), loaded);
    }
}

pub(super) fn tile_bounds(offset: usize) -> [f32; 4] {
    let col = f32::from(u16::try_from(offset % 4).unwrap_or(0));
    let row = f32::from(u16::try_from(offset / 4).unwrap_or(0));
    [
        col.mul_add(85.0, 123.0),
        row.mul_add(72.0, 77.0),
        80.0,
        60.0,
    ]
}

impl App {
    pub(super) fn browser_page(&self) -> Option<(Vec<String>, usize)> {
        match self.popup {
            Some(Popup::DoorAssets) => Some((self.door_choices(), 0)),
            Some(Popup::Library { materials, page }) => Some((self.library(materials), page)),
            Some(Popup::GameplayChoices {
                choice: choice @ (GameplayChoice::EntityModel | GameplayChoice::Fixture),
                page,
            }) => Some((self.gameplay_choices(choice), page)),
            _ => None,
        }
    }

    pub(super) fn door_choices(&self) -> Vec<String> {
        DOOR_PREVIEWS
            .into_iter()
            .filter(|id| {
                format!("{} {}", id, door_name(id))
                    .to_lowercase()
                    .contains(&self.browser.query.to_lowercase())
            })
            .map(str::to_owned)
            .collect()
    }

    pub(super) fn browser_themes(&self) -> &[String] {
        &self.browser.themes
    }

    pub(super) fn browser_matches(&self, entry: &AssetEntry) -> bool {
        let theme = self.browser.themes.get(self.browser.theme);
        (self.browser.theme == 0
            || theme.is_some_and(|t| {
                t == entry
                    .theme
                    .as_ref()
                    .map_or("Shared", |value| value.as_str())
            }))
            && format!("{} {}", entry.id, entry.display_name)
                .to_lowercase()
                .contains(&self.browser.query.to_lowercase())
    }

    pub(super) fn browser_images(&mut self) -> Vec<UiImage> {
        let Some((ids, page)) = self.browser_page() else {
            return Vec::new();
        };
        let visible: Vec<_> = ids
            .into_iter()
            .skip(page.saturating_mul(PAGE_SIZE))
            .take(PAGE_SIZE)
            .collect();
        self.browser.prepare_one(&visible);
        visible
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                Some(UiImage {
                    bounds: tile_bounds(index),
                    image: Arc::clone(self.browser.image(id)?),
                })
            })
            .collect()
    }

    pub(super) fn browser_theme(&mut self) {
        self.browser.theme = self
            .browser
            .theme
            .saturating_add(1)
            .checked_rem(self.browser_themes().len().max(1))
            .unwrap_or(0);
        self.reset_browser_page();
    }

    const fn reset_browser_page(&mut self) {
        if let Some(
            Popup::Library { page, materials: _ } | Popup::GameplayChoices { page, choice: _ },
        ) = &mut self.popup
        {
            *page = 0;
        }
        self.ui_focus = None;
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Asset search consumes only its text, paging and scrolling events."
    )]
    pub(super) fn browser_event(&mut self, event: &sdl3::event::Event) -> bool {
        use sdl3::{event::Event, keyboard::Keycode};
        if self.browser_page().is_none() {
            return false;
        }
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if self.browser.query.len().saturating_add(text.len()) <= 96 {
                    self.browser.query.push_str(text);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::A),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if super::input::command(*keymod) => self.browser.query.clear(),
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                let _removed = self.browser.query.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Left),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                self.page_library(false);
                return true;
            }
            Event::KeyDown {
                keycode: Some(Keycode::Right),
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                self.page_library(true);
                return true;
            }
            Event::MouseWheel {
                y,
                timestamp: _,
                window_id: _,
                which: _,
                x: _,
                direction: _,
                mouse_x: _,
                mouse_y: _,
                integer_x: _,
                integer_y: _,
            } => {
                self.page_library(*y < 0.0);
                return true;
            }
            _ => return false,
        }
        self.reset_browser_page();
        true
    }
}

pub(super) fn choice_action(choice: Option<GameplayChoice>, index: usize) -> Action {
    choice.map_or(Action::ChooseAsset(index), |c| Action::GameChoose(c, index))
}

pub(super) fn door_name(id: &str) -> &str {
    match id {
        "editor:door_interior" => "Interior door",
        "editor:door_sauna" => "Sauna door",
        "editor:door_restroom" => "Restroom door",
        _ => id,
    }
}

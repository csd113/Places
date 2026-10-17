//! Opt-in console with bounded storage, typed commands and cached native UI.
pub mod command;
pub mod runtime;
mod ui;

use crate::engine::render::Vertex;
use command::Command;
use sdl3::event::Event;
use sdl3::keyboard::Keycode;
use std::collections::VecDeque;

const MAX_INPUT: usize = 256;
const MAX_HISTORY: usize = 32;
const MAX_OUTPUT: usize = 128;
const MAX_MESSAGE: usize = 512;
pub(super) const COLUMNS: usize = 69;
pub(super) const VISIBLE_LINES: usize = 15;

#[derive(Default)]
pub struct Console {
    open: bool,
    input: String,
    output: VecDeque<String>,
    history: VecDeque<String>,
    history_index: Option<usize>,
    draft: String,
    scroll: usize,
    dirty: bool,
    vertices: Vec<Vertex>,
}

impl Console {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// Disabled and menu states cannot open the console.
    pub fn toggle(&mut self, enabled: bool, playing: bool) {
        if !enabled || !playing {
            self.close();
            return;
        }
        self.open = !self.open;
        self.dirty = true;
        if self.open && self.output.is_empty() {
            self.write("Developer console. /help lists commands.");
        }
    }

    pub fn latest_message(&self) -> Option<&str> {
        self.output.back().map(String::as_str)
    }

    #[must_use]
    pub const fn scroll_offset(&self) -> usize {
        self.scroll
    }

    pub fn close(&mut self) {
        self.open = false;
        self.input.clear();
        self.draft.clear();
        self.history_index = None;
        self.dirty = true;
    }

    pub fn write(&mut self, message: impl AsRef<str>) {
        if self.output.len() == MAX_OUTPUT {
            drop(self.output.pop_front());
        }
        self.output.push_back(
            message
                .as_ref()
                .chars()
                .filter(|ch| !ch.is_control())
                .take(MAX_MESSAGE)
                .collect(),
        );
        self.scroll = 0;
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        self.output.clear();
        self.scroll = 0;
        self.dirty = true;
    }

    fn append_text(&mut self, text: &str) {
        // The native font is ASCII. Reject unsupported glyphs and the toggle
        // key's text event instead of accepting invisible command characters.
        for ch in text
            .chars()
            .filter(|ch| ch.is_ascii() && !ch.is_control() && !matches!(ch, '`' | '~'))
        {
            if self.input.len() >= MAX_INPUT {
                break;
            }
            self.input.push(ch);
        }
        self.history_index = None;
        self.dirty = true;
    }

    fn recall(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        if older {
            if self.history_index.is_none() {
                self.draft.clone_from(&self.input);
            }
            let index = self.history_index.map_or_else(
                || self.history.len().saturating_sub(1),
                |index| index.saturating_sub(1),
            );
            self.history_index = Some(index);
            self.input = self.history.get(index).cloned().unwrap_or_default();
        } else if let Some(index) = self.history_index {
            let next = index.saturating_add(1);
            if next < self.history.len() {
                self.history_index = Some(next);
                self.input = self.history.get(next).cloned().unwrap_or_default();
            } else {
                self.history_index = None;
                self.input.clone_from(&self.draft);
            }
        }
        self.dirty = true;
    }

    /// Consumes text/editing input only while open; callers gate gameplay.
    pub fn handle_event(&mut self, event: &Event) -> Option<Result<Command, String>> {
        if !self.open {
            return None;
        }
        if let Event::TextInput {
            text,
            timestamp: _,
            window_id: _,
        } = event
        {
            self.append_text(text);
        }
        if let Event::MouseWheel {
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
        } = event
        {
            self.scroll(*y > 0.0);
        }
        if let Event::KeyDown {
            keycode: Some(key),
            repeat,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
        } = event
        {
            if *key == Keycode::Backspace {
                let _character = self.input.pop();
                self.dirty = true;
            }
            if *key == Keycode::Up {
                self.recall(true);
            }
            if *key == Keycode::Down {
                self.recall(false);
            }
            if *key == Keycode::PageUp {
                self.scroll(true);
            }
            if *key == Keycode::PageDown {
                self.scroll(false);
            }
            if *key == Keycode::Return && !repeat && !self.input.trim().is_empty() {
                let line = std::mem::take(&mut self.input);
                self.write(format!("> {line}"));
                if self.history.back() != Some(&line) {
                    if self.history.len() == MAX_HISTORY {
                        drop(self.history.pop_front());
                    }
                    self.history.push_back(line.clone());
                }
                self.history_index = None;
                self.draft.clear();
                return Some(command::parse(&line));
            }
        }
        None
    }

    fn scroll(&mut self, older: bool) {
        let maximum = self.wrapped_lines().len().saturating_sub(VISIBLE_LINES);
        self.scroll = if older {
            self.scroll.saturating_add(3).min(maximum)
        } else {
            self.scroll.saturating_sub(3)
        };
        self.dirty = true;
    }

    fn wrapped_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for message in &self.output {
            let chars: Vec<char> = message.chars().collect();
            if chars.is_empty() {
                lines.push(String::new());
            }
            for chunk in chars.chunks(COLUMNS) {
                lines.push(chunk.iter().collect());
            }
        }
        lines
    }

    pub fn vertices(&mut self, noclip: bool) -> &[Vertex] {
        if self.dirty {
            self.vertices = ui::geometry(self, noclip);
            self.dirty = false;
        }
        &self.vertices
    }
}

#[must_use]
pub fn toggle_key(event: &Event, binding: &str) -> bool {
    crate::game::input::pressed_key(event)
        .is_some_and(|key| crate::game::input::keycode_to_str(key) == binding)
}

#[must_use]
pub const fn escape_key(event: &Event) -> bool {
    matches!(
        event,
        Event::KeyDown {
            keycode: Some(Keycode::Escape),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _
        }
    )
}

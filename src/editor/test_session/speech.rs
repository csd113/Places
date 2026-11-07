//! Embedded Test reuses player speech while keeping explicit guidance modal.
use crate::game::ui::Subtitles;
use sdl3::keyboard::Keycode;
use std::collections::VecDeque;

pub(super) struct Message {
    pub title: String,
    pub rows: Vec<String>,
    pub page: usize,
    pub completed: bool,
}

#[derive(Default)]
pub(super) struct Speech {
    pub modal: VecDeque<Message>,
    pub subtitles: Subtitles,
}
impl Speech {
    /// Returns true only when gameplay must pause for an intentional modal.
    pub fn show(&mut self, title: String, lines: &[String], completed: bool) -> bool {
        if self.modal.front().is_some_and(|message| message.completed) {
            return false;
        }
        if completed {
            self.clear();
        } else if !self.modal.is_empty() {
            return false;
        } else if self.subtitles.guidance_requested.is_none() {
            self.subtitles.push(title, lines);
            return false;
        }
        self.modal.push_back(Message {
            title,
            rows: crate::game::ui::wrap_guidance(lines),
            page: 0,
            completed,
        });
        true
    }
    /// Hint intent applies to exactly one command batch, even when it is empty.
    pub const fn finish_commands(&mut self) {
        self.subtitles.guidance_requested = None;
    }
    pub fn clear(&mut self) {
        self.modal.clear();
        self.subtitles.clear();
    }
    pub fn dismiss_hint(&mut self) {
        drop(self.modal.pop_front());
        self.subtitles.guidance_requested = None;
    }
    pub fn tick(&mut self, seconds: f32) {
        if self.modal.is_empty() {
            self.subtitles.tick(seconds);
        }
    }
    pub fn navigate(&mut self, key: Keycode, cinematic: bool) -> bool {
        if !self.modal.is_empty() || !self.subtitles.active() {
            return false;
        }
        self.subtitles.navigate(key, cinematic)
    }
}

#[cfg(test)]
#[path = "../tests/subtitle_routing.rs"]
mod tests;

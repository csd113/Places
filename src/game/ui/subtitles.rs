//! Non-modal speech shares the ordinary text/rectangle pipeline.
use super::{add_rect_rgba, draw_text, menu, row_y, text_width};
use crate::engine::render::Vertex;
use std::collections::VecDeque;

#[derive(Debug)]
struct Speech {
    speaker: String,
    rows: Vec<String>,
    page: usize,
    elapsed: f32,
}

/// Keeps consecutive messages in order; pausing never spends their reading time.
#[derive(Debug, Default)]
pub struct Subtitles {
    pub guidance_requested: Option<()>,
    queue: VecDeque<Speech>,
}
impl Subtitles {
    pub fn clear(&mut self) {
        self.guidance_requested = None;
        self.queue.clear();
    }
    pub fn push(&mut self, speaker: String, lines: &[String]) {
        let rows = menu::wrap(&lines.join("\n"), 54);
        // Held/repeated interactions must not build a backlog of the same speech.
        if self
            .queue
            .back()
            .is_some_and(|last| last.speaker == speaker && last.rows == rows)
        {
            return;
        }
        self.queue.push_back(Speech {
            speaker,
            rows,
            page: 0,
            elapsed: 0.0,
        });
    }
    pub fn advance(&mut self) {
        let Some(speech) = self.queue.front_mut() else {
            return;
        };
        if speech.page.saturating_add(1).saturating_mul(2) < speech.rows.len() {
            speech.page = speech.page.saturating_add(1);
            speech.elapsed = 0.0;
        } else {
            drop(self.queue.pop_front());
        }
    }
    pub fn previous(&mut self) {
        if let Some(speech) = self.queue.front_mut() {
            speech.page = speech.page.saturating_sub(1);
            speech.elapsed = 0.0;
        }
    }
    pub fn skip(&mut self) {
        drop(self.queue.pop_front());
    }
    /// Cinematics expose only forward dialogue; their shell owns closing.
    pub fn navigate(&mut self, key: sdl3::keyboard::Keycode, cinematic: bool) -> bool {
        use sdl3::keyboard::Keycode;
        if !self.active() {
            return false;
        }
        if key == Keycode::Return || key == Keycode::PageDown {
            self.advance();
        } else if !cinematic && key == Keycode::PageUp {
            self.previous();
        } else if !cinematic && key == Keycode::Backspace {
            self.skip();
        } else {
            return false;
        }
        true
    }
    pub fn active(&self) -> bool {
        !self.queue.is_empty()
    }
    pub fn tick(&mut self, delta: f32) {
        let Some(speech) = self.queue.front_mut() else {
            return;
        };
        let characters = speech
            .rows
            .iter()
            .skip(speech.page.saturating_mul(2))
            .take(2)
            .map(|row| row.chars().count())
            .sum::<usize>();
        let duration = f32::from(u16::try_from(characters).unwrap_or(u16::MAX))
            .mul_add(0.06, 2.0)
            .clamp(3.5, 9.0);
        speech.elapsed += delta.clamp(0.0, 0.1);
        if speech.elapsed >= duration {
            self.advance();
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"pending":self.queue.len(),"current":self.queue.front().map(|s| serde_json::json!({"speaker":s.speaker,"rows":s.rows,"page":s.page,"elapsed":s.elapsed}))})
    }
    /// Borrow the visible dialogue for a shell using its own font atlas.
    pub fn current(&self) -> Option<(&str, &[String])> {
        let speech = self.queue.front()?;
        let remaining = speech
            .rows
            .get(speech.page.saturating_mul(2)..)
            .unwrap_or_default();
        let rows = remaining.get(..remaining.len().min(2)).unwrap_or_default();
        Some((&speech.speaker, rows))
    }
    pub fn append(&self, vertices: &mut Vec<Vertex>) {
        let Some(speech) = self.queue.front() else {
            return;
        };
        let rows = speech
            .rows
            .iter()
            .skip(speech.page.saturating_mul(2))
            .take(2);
        let speaker_scale = (378.0 / text_width(&speech.speaker, 1.0).max(1.0)).min(0.625);
        let content_width = rows
            .clone()
            .map(|row| text_width(row, 0.875))
            .fold(text_width(&speech.speaker, speaker_scale), f32::max);
        let half_width = (content_width + 20.0).max(112.0) * 0.5;
        let height = if rows.len() > 1 { 37.0 } else { 27.0 };
        let top = 244.0 - height;
        add_rect_rgba(
            vertices,
            240.0 - half_width,
            top,
            240.0 + half_width,
            244.0,
            [0.012, 0.018, 0.023, 0.72],
        );
        draw_text(
            vertices,
            &speech.speaker,
            (480.0 - text_width(&speech.speaker, speaker_scale)) * 0.5,
            top + 5.0,
            speaker_scale,
            [0.93, 0.83, 0.58],
        );
        for (index, row) in rows.enumerate() {
            draw_text(
                vertices,
                row,
                (480.0 - text_width(row, 0.875)) * 0.5,
                row_y(index, 10.0, top + 15.0),
                0.875,
                [1.0, 0.97, 0.90],
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/subtitles.rs"]
mod tests;

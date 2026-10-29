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
    pub fn append(&self, vertices: &mut Vec<Vertex>) {
        let Some(speech) = self.queue.front() else {
            return;
        };
        add_rect_rgba(
            vertices,
            40.0,
            185.0,
            440.0,
            244.0,
            [0.012, 0.018, 0.023, 0.72],
        );
        let speaker_scale = (378.0 / text_width(&speech.speaker, 1.0).max(1.0)).min(0.75);
        draw_text(
            vertices,
            &speech.speaker,
            (480.0 - text_width(&speech.speaker, speaker_scale)) * 0.5,
            193.0,
            speaker_scale,
            [0.93, 0.83, 0.58],
        );
        for (index, row) in speech
            .rows
            .iter()
            .skip(speech.page.saturating_mul(2))
            .take(2)
            .enumerate()
        {
            draw_text(
                vertices,
                row,
                (480.0 - text_width(row, 0.875)) * 0.5,
                row_y(index, 12.0, 205.0),
                0.875,
                [1.0, 0.97, 0.90],
            );
        }
        let hint = "Enter: next   PgUp: back   Backspace: skip";
        draw_text(
            vertices,
            hint,
            (480.0 - text_width(hint, 0.625)) * 0.5,
            233.0,
            0.625,
            [0.64, 0.68, 0.70],
        );
    }
}

#[cfg(test)]
#[path = "../tests/subtitles.rs"]
mod tests;

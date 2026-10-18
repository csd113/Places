//! One preloaded, listener-centred chooser sample, capped at three seconds.

use std::time::{Duration, Instant};

use super::App;
use crate::engine::audio::{AudioSettings, Listener, PlayRequest};

impl App {
    pub(super) fn preview_sound(&mut self, index: usize) -> Result<(), String> {
        let cue = self
            .sound_cues
            .get(index)
            .ok_or("Choose a built-in sound cue")?;
        self.preview_audio.clear_world();
        if !self.preview_audio.play(
            cue,
            PlayRequest {
                spatial: Some(false),
                looping: Some(false),
                gain: 0.35,
                ..PlayRequest::default()
            },
        ) {
            return Err("Sound preview could not start; check the audio output device".into());
        }
        self.sound_preview_until = Instant::now().checked_add(Duration::from_secs(3));
        self.status = format!("Previewing {cue} (quiet, at most 3 seconds).");
        Ok(())
    }

    pub(super) fn update_audio_preview(&mut self) {
        if self
            .sound_preview_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.preview_audio.clear_world();
            self.sound_preview_until = None;
        }
        self.preview_audio
            .update(Listener::default(), false, 1.0, AudioSettings::default());
    }
}

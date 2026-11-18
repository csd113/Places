//! Editor audio preparation and the short listener-centred chooser preview.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::App;
use crate::engine::audio::{AudioSettings, AudioSystem, Listener, PlayRequest, PreparedAudio};

pub(super) struct StartupAudio {
    asset_root: &'static Path,
    cancelled: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<PreparedAudio, String>>>,
}

impl StartupAudio {
    pub(super) fn start(asset_root: &'static Path) -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let worker = thread::Builder::new()
            .name("places-editor-audio-prepare".into())
            .spawn(move || {
                AudioSystem::prepare(asset_root, || worker_cancelled.load(Ordering::Relaxed))
            })
            .ok();
        Self {
            asset_root,
            cancelled,
            worker,
        }
    }

    pub(super) fn finish(
        mut self,
        sdl: &sdl3::Sdl,
        pump: &mut impl FnMut() -> Result<(), String>,
    ) -> Result<AudioSystem, String> {
        if let Some(worker) = &self.worker {
            while !worker.is_finished() {
                pump()?;
                thread::sleep(Duration::from_millis(2));
            }
        }
        let prepared = self.worker.take().map_or_else(
            || AudioSystem::prepare(self.asset_root, || false),
            |worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| Err("Editor audio preparation panicked".into()))
            },
        );
        Ok(AudioSystem::from_prepared(prepared, sdl))
    }
}

impl Drop for StartupAudio {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.cancelled.store(true, Ordering::Relaxed);
            let _retired = worker.join();
        }
    }
}

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

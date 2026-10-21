//! Explicit native benchmark recording: consecutive rendered RGBA frames flow
//! to a caller-owned FIFO/encoder. Ordinary gameplay allocates no recorder.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::engine::materials::RawImage;

pub struct Recording {
    path: PathBuf,
    sink: Option<File>,
    size: (u32, u32),
    remaining: u16,
    captured: u16,
    stride: u16,
    start: u64,
    source_frame: u64,
    interval: Option<std::time::Duration>,
    next_capture: std::time::Instant,
}

pub struct VideoFrame {
    pub captured: u16,
    pub source_frame: u64,
    pub finished: bool,
}

impl Recording {
    pub(crate) fn from_env() -> Option<Self> {
        if std::env::var("PLACES_BENCH").as_deref() != Ok("1") {
            return None;
        }
        let path = PathBuf::from(std::env::var_os("PLACES_BENCH_VIDEO_PIPE")?);
        match Self::configure(path) {
            Ok(recording) => Some(recording),
            Err(error) => {
                crate::engine::logging::warn(format!("[video] {error}"));
                None
            }
        }
    }

    fn configure(path: PathBuf) -> Result<Self, String> {
        let size = std::env::var("PLACES_BENCH_VIDEO_SIZE")
            .map_err(|error| format!("native video requires its exact drawable size: {error}"))?;
        let (width_text, height_text) = size
            .split_once('x')
            .ok_or("native video size must be WIDTHxHEIGHT")?;
        let width = width_text
            .parse::<u32>()
            .map_err(|error| format!("invalid video width: {error}"))?;
        let height = height_text
            .parse::<u32>()
            .map_err(|error| format!("invalid video height: {error}"))?;
        if !(64..=4096).contains(&width)
            || !(64..=4096).contains(&height)
            || width
                .checked_mul(height)
                .and_then(|pixels| pixels.checked_mul(4))
                .is_none_or(|bytes| bytes > 16_777_216)
        {
            return Err("native video frames must fit within 16 MiB".into());
        }
        if !path.is_absolute() || !path.starts_with(&crate::engine::paths::user()?.data) {
            return Err("native video FIFO must be inside the explicit state directory".into());
        }
        validate_fifo(&path)?;
        Ok(Self {
            path,
            sink: None,
            size: (width, height),
            remaining: bounded_env("PLACES_BENCH_VIDEO_FRAMES", 600, 1, 1800)?,
            captured: 0,
            stride: bounded_env("PLACES_BENCH_VIDEO_STRIDE", 2, 1, 4)?,
            start: u64::from(bounded_env("PLACES_BENCH_VIDEO_START_FRAME", 0, 0, 3600)?),
            source_frame: 0,
            interval: if std::env::var_os("PLACES_BENCH_VIDEO_INTERVAL_MS").is_some() {
                Some(std::time::Duration::from_millis(u64::from(bounded_env(
                    "PLACES_BENCH_VIDEO_INTERVAL_MS",
                    500,
                    100,
                    5000,
                )?)))
            } else {
                None
            },
            next_capture: std::time::Instant::now(),
        })
    }

    /// Reads only selected ready frames, preserving the actual renderer's
    /// orientation and RGBA bytes. The caller chooses a matching encoder rate.
    pub(crate) fn capture(
        &mut self,
        read_frame: impl FnOnce() -> Result<RawImage, String>,
    ) -> Result<Option<VideoFrame>, String> {
        self.source_frame = self.source_frame.saturating_add(1);
        if self.remaining == 0
            || self.source_frame <= self.start
            || !self
                .source_frame
                .saturating_sub(self.start)
                .saturating_sub(1)
                .is_multiple_of(u64::from(self.stride))
        {
            return Ok(None);
        }
        if let Some(interval) = self.interval {
            let now = std::time::Instant::now();
            if now < self.next_capture {
                return Ok(None);
            }
            self.next_capture = now
                .checked_add(interval)
                .ok_or("native video deadline overflow")?;
        }
        let image = read_frame()?;
        if (image.width, image.height) != self.size
            || image
                .width
                .checked_mul(image.height)
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                != Some(image.rgba.len())
        {
            return Err("native video drawable changed or returned malformed RGBA".into());
        }
        if self.sink.is_none() {
            validate_fifo(&self.path)?;
            self.sink = Some(
                OpenOptions::new()
                    .write(true)
                    .open(&self.path)
                    .map_err(|error| format!("cannot open native video FIFO: {error}"))?,
            );
        }
        let sink = self.sink.as_mut().ok_or("native video sink is missing")?;
        sink.write_all(&image.rgba)
            .map_err(|error| format!("native video encoder write failed: {error}"))?;
        self.remaining = self.remaining.saturating_sub(1);
        self.captured = self.captured.saturating_add(1);
        let finished = self.remaining == 0;
        if finished {
            self.sink = None;
        }
        Ok(Some(VideoFrame {
            captured: self.captured,
            source_frame: self.source_frame,
            finished,
        }))
    }
}

fn bounded_env(name: &str, default: u16, minimum: u16, maximum: u16) -> Result<u16, String> {
    let value = match std::env::var(name) {
        Ok(text) => text
            .parse::<u16>()
            .map_err(|error| format!("invalid {name}: {error}"))?,
        Err(std::env::VarError::NotPresent) => default,
        Err(std::env::VarError::NotUnicode(_)) => return Err(format!("invalid {name}")),
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(format!("{name} must be {minimum}..={maximum}"));
    }
    Ok(value)
}

#[cfg(unix)]
fn validate_fifo(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("native video FIFO is unavailable: {error}"))?;
    if metadata.file_type().is_fifo() {
        Ok(())
    } else {
        Err("native video output must be a caller-created FIFO".into())
    }
}

#[cfg(not(unix))]
fn validate_fifo(_path: &std::path::Path) -> Result<(), String> {
    Err("native video FIFO capture is currently supported on Unix hosts".into())
}

impl crate::game::app::FrameLoop<'_> {
    pub(crate) fn record_video_frame(&mut self, frame_begin: std::time::Instant) {
        let Some(recording) = self.video.as_mut() else {
            return;
        };
        let audio_output_frames = self.audio.system.stats().output_frames;
        let result = recording.capture(|| self.renderer.capture_default_framebuffer());
        // Only this explicit recorder paces ready frames to 60 Hz. The native
        // action clock and fixed 1/60 simulation then match the 30 fps stream
        // sampled every other frame, without inserting or duplicating frames.
        let pace = recording.interval.is_none();
        let pause =
            std::time::Duration::from_nanos(16_666_667).saturating_sub(frame_begin.elapsed());
        if pace && !pause.is_zero() {
            std::thread::sleep(pause);
        }
        match result {
            Ok(Some(frame)) => {
                self.trace.record("native_video_frame", self.load_generation, &serde_json::json!({"captured":frame.captured,"source_ready_frame":frame.source_frame,"audio_output_frames":audio_output_frames}).to_string());
                if frame.finished {
                    self.video = None;
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.video = None;
                self.fatal_error = Some(error);
                self.game.stop();
            }
        }
    }
}

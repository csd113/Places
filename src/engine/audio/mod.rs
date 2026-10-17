//! Bounded preloaded game audio, independent of level and player ownership.
//!
//! Native playback uses Kira's maintained mixer on the existing SDL3 platform.
//! WAV files are validated and decoded once at loading time. Playback only
//! shares their samples and creates small bounded voice controls. Spatial audio
//! uses listener-relative stereo panning and distance attenuation, without HRTF
//! or geometry occlusion. Browser builds deliberately remain silent.

mod catalog;
#[cfg(not(target_arch = "wasm32"))]
mod native;

use std::path::Path;

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub use catalog::AudioCatalog;

/// Maximum simultaneously live mixer voices, including loops.
pub const MAX_VOICES: usize = 48;
/// Maximum decoded sample memory retained across every cue and variant.
pub const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;

/// Which simulation clock controls a sound's speed and pitch.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeDomain {
    /// Follows world simulation speed and pauses when that speed reaches zero.
    #[default]
    World,
    /// Retains normal speed at zero world time; application pause still stops it.
    Real,
}

/// Independently adjustable volume category.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioBus {
    /// Player actions, props, doors and authored effects.
    #[default]
    Effects,
    /// Continuous environmental beds and regional ambience.
    Ambient,
}

/// Application-owned stable identity for a looping or moving source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceId(pub u64);

/// One preloaded cue playback request. No paths are accepted at playback time.
#[derive(Clone, Copy, Debug)]
pub struct PlayRequest {
    /// Optional source identity for replacement, movement and explicit stop.
    pub source: Option<SourceId>,
    /// World metres; absent means listener-centred playback.
    pub position: Option<Vec3>,
    /// Override the cue's positional/ambient mode for a standalone emitter.
    pub spatial: Option<bool>,
    /// Explicit loop override; absent uses the catalog's authored default.
    pub looping: Option<bool>,
    /// Finite linear amplitude in `0..=1`.
    pub gain: f32,
    /// Playback clock policy.
    pub time_domain: TimeDomain,
    /// Volume category.
    pub bus: AudioBus,
}

impl Default for PlayRequest {
    fn default() -> Self {
        Self {
            source: None,
            position: None,
            spatial: None,
            looping: None,
            gain: 1.0,
            time_domain: TimeDomain::World,
            bus: AudioBus::Effects,
        }
    }
}

/// Listener transform in the same world metres as emitter positions.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    /// Ear/eye position.
    pub position: Vec3,
    /// Forward direction; normalized after validation.
    pub forward: Vec3,
    /// Up direction; forward and up must not be parallel.
    pub up: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::NEG_Z,
            up: Vec3::Y,
        }
    }
}

/// Application settings, applied immediately to active and future voices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioSettings {
    /// Master linear amplitude in `0..=1`.
    pub master_volume: f32,
    /// Effects category linear amplitude in `0..=1`.
    pub effects_volume: f32,
    /// Ambience category linear amplitude in `0..=1`.
    pub ambient_volume: f32,
    /// Silences output while preserving voice positions and loop ownership.
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            effects_volume: 1.0,
            ambient_volume: 0.6,
            muted: false,
        }
    }
}

/// Bounded diagnostic counters; obtaining a snapshot never touches a device.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct AudioStats {
    /// Whether a native output stream was initialized successfully.
    pub enabled: bool,
    /// Number of decoded files shared by every voice.
    pub loaded_clips: usize,
    /// Retained stereo float sample bytes.
    pub decoded_bytes: usize,
    /// Active controls, including paused voices.
    pub active_voices: usize,
    /// Loop starts waiting for stopped mixer slots to retire.
    pub pending_voices: usize,
    /// Accepted playback requests since initialization.
    pub played: u64,
    /// Rejected invalid, unknown or over-capacity requests.
    pub dropped: u64,
    /// Loading, device or callback failures.
    pub errors: u64,
    /// Stereo frames actually mixed by the device callback.
    pub output_frames: u64,
    /// Largest absolute mixed sample since initialization, before SDL conversion.
    pub peak_amplitude: f32,
    /// Developer tap frames discarded when its fixed queue was full.
    pub capture_dropped: u64,
}

/// Explicit developer snapshot of one active or queued voice. Constructing
/// snapshots allocates only when the caller requests this diagnostic.
#[derive(Clone, Debug, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "diagnostics record independent spatial, loop, pause and queue facts rather than one state machine"
)]
pub struct AudioVoiceSnapshot {
    /// Logical cue ID.
    pub cue: String,
    /// Application source identity.
    pub source: Option<u64>,
    /// Current world position, or listener-centred playback.
    pub position: Option<[f32; 3]>,
    /// Actual positional/ambient mixer mode, including authored overrides.
    pub spatial: bool,
    /// Linear target amplitude after distance, categories, mute and pause.
    pub gain: f32,
    /// Stereo balance in `-1..=1`.
    pub pan: f32,
    /// Speed/pitch multiplier.
    pub playback_rate: f64,
    /// Whether playback repeats the complete sample.
    pub looping: bool,
    /// Clock policy.
    pub time_domain: TimeDomain,
    /// Whether application pause or this sound's clock currently pauses it.
    pub paused: bool,
    /// Whether the start is queued behind stopped mixer voices.
    pub pending: bool,
}

/// Reusable audio owner. Dropping it closes the stream before decoded data.
pub struct AudioSystem {
    #[cfg(not(target_arch = "wasm32"))]
    native: Option<Box<native::NativeAudio>>,
    fallback_stats: AudioStats,
}

impl AudioSystem {
    /// Initializes and preloads `asset_root/audio/catalog.json` during loading.
    /// Device/library failure and `PLACES_AUDIO_DISABLE=1` produce silent audio.
    #[must_use]
    pub fn new(asset_root: &Path, sdl: &sdl3::Sdl) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if std::env::var("PLACES_AUDIO_DISABLE").is_ok_and(|value| value == "1") {
                return Self {
                    native: None,
                    fallback_stats: AudioStats::default(),
                };
            }
            match native::NativeAudio::load(asset_root, sdl) {
                Ok(audio) => Self {
                    native: Some(Box::new(audio)),
                    fallback_stats: AudioStats::default(),
                },
                Err(error) => {
                    crate::engine::logging::warn_once(
                        "audio-init",
                        format!("[audio] silent fallback: {error}"),
                    );
                    Self {
                        native: None,
                        fallback_stats: AudioStats {
                            errors: 1,
                            ..AudioStats::default()
                        },
                    }
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (asset_root, sdl);
            Self {
                fallback_stats: AudioStats::default(),
            }
        }
    }

    /// Plays an already decoded cue. Returns false for a rejected/silent request.
    pub fn play(&mut self, cue: &str, request: PlayRequest) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            return audio.play(cue, request);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (cue, request);
        self.fallback_stats.dropped = self.fallback_stats.dropped.saturating_add(1);
        false
    }

    /// Updates spatial mix and clocks. Invalid transforms retain the last valid
    /// listener. Volumes clamp to `0..=1`, world speed to `0..=4`; nonfinite
    /// volume/speed values become zero.
    pub fn update(
        &mut self,
        listener: Listener,
        paused: bool,
        world_scale: f32,
        settings: AudioSettings,
    ) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            audio.update(listener, paused, world_scale, settings);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (listener, paused, world_scale, settings);
    }

    /// Moves every active voice belonging to a source; ignores nonfinite poses.
    pub fn set_source_position(&mut self, source: SourceId, position: Vec3) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            audio.set_source_position(source, position);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (source, position);
    }

    /// Resolves only currently active source identities. A missing source is
    /// stopped immediately, so despawned objects cannot retain orphaned loops.
    pub fn update_source_positions(&mut self, mut resolve: impl FnMut(SourceId) -> Option<Vec3>) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            audio.update_source_positions(&mut resolve);
        }
        let _ = &mut resolve;
    }

    /// Stops all voices owned by one source, including paused loops.
    pub fn stop_source(&mut self, source: SourceId) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            audio.stop_source(source);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = source;
    }

    /// Stops all playback, retaining the shared library for the next map.
    pub fn clear_world(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            audio.clear_world();
        }
    }

    /// Closes playback and releases the decoded library. Safe to call repeatedly.
    pub fn unload(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.native = None;
        }
    }

    /// Current playback/resource counters and observed device output.
    #[must_use]
    pub fn stats(&self) -> AudioStats {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &self.native {
            return audio.stats();
        }
        self.fallback_stats
    }

    /// Allocates a bounded snapshot only for an explicit developer inspection.
    #[must_use]
    pub fn voice_snapshot(&self) -> Vec<AudioVoiceSnapshot> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &self.native {
            return audio.voice_snapshot();
        }
        Vec::new()
    }

    /// Drains the optional bounded PCM tap into caller-owned stereo frames.
    /// Enable `PLACES_AUDIO_CAPTURE=1` before initialization; sample rate is
    /// 48 kHz. This never records a microphone and never writes a file.
    pub fn drain_capture(&mut self, output: &mut [[f32; 2]]) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(audio) = &mut self.native {
            return audio.drain_capture(output);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = output;
        0
    }
}

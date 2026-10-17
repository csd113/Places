use std::ffi::{c_int, c_void};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use glam::Vec3;
use kira::backend::{Backend, Renderer};
use kira::sound::PlaybackState;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::track::MainTrackBuilder;
use kira::{AudioManager, AudioManagerSettings, Capacities, Decibels, Tween};
use rtrb::{Consumer, Producer, RingBuffer};
use sdl3::sys::audio::{self, SDL_AudioStream};

use super::catalog::{MAX_FILE_BYTES, valid_gain};
use super::{
    AudioBus, AudioCatalog, AudioSettings, AudioStats, AudioVoiceSnapshot, Listener,
    MAX_DECODED_BYTES, MAX_VOICES, PlayRequest, SourceId, TimeDomain,
};

#[cfg(test)]
#[path = "../tests/audio_backend.rs"]
mod tests;

const OUTPUT_RATE: u32 = 48_000;
const OUTPUT_RATE_I32: i32 = 48_000;
const CALLBACK_SAMPLES: usize = 2048;
const MAX_CALLBACK_SAMPLES: usize = 16_384;
const MAX_CLIP_SECONDS: u32 = 30;
const CAPTURE_FRAMES: usize = 96_000;

#[derive(Default)]
struct OutputCounters {
    frames: AtomicU64,
    errors: AtomicU64,
    peak: AtomicU32,
    capture_dropped: AtomicU64,
    resource_gate: AtomicBool,
}

/// Kira 0.12.5 frees arena slots before publishing retired sounds, while play
/// reserves then drains retired sounds. Interleaving those boundaries can fill
/// its retired queue and panic on the next retirement. Exclude only publication
/// and `on_start_processing` without waiting: a busy callback keeps rendering
/// existing sounds, and a busy gameplay start follows bounded retry/drop policy.
struct ResourceGate<'a>(&'a AtomicBool);

impl<'a> ResourceGate<'a> {
    fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_previous| Self(flag))
    }
}

impl Drop for ResourceGate<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn resource_boundary(gate: &AtomicBool, boundary: impl FnOnce()) {
    let Some(_guard) = ResourceGate::try_acquire(gate) else {
        return;
    };
    boundary();
}

struct OutputCallback {
    renderer: Renderer,
    scratch: Box<[f32]>,
    counters: Arc<OutputCounters>,
    capture: Option<Producer<[f32; 2]>>,
}

struct SdlSettings {
    subsystem: sdl3::AudioSubsystem,
    capture: bool,
}

/// The stream is destroyed before callback storage, so SDL cannot race freed
/// callback userdata. SDL's logical default device follows device changes.
struct SdlBackend {
    _subsystem: sdl3::AudioSubsystem,
    stream: Option<NonNull<SDL_AudioStream>>,
    callback: Option<NonNull<OutputCallback>>,
    counters: Arc<OutputCounters>,
    capture: Option<Consumer<[f32; 2]>>,
    capture_enabled: bool,
}

impl Backend for SdlBackend {
    type Settings = SdlSettings;
    type Error = String;

    fn setup(settings: Self::Settings, _buffer_size: usize) -> Result<(Self, u32), String> {
        Ok((
            Self {
                _subsystem: settings.subsystem,
                stream: None,
                callback: None,
                counters: Arc::new(OutputCounters::default()),
                capture: None,
                capture_enabled: settings.capture,
            },
            OUTPUT_RATE,
        ))
    }

    fn start(&mut self, renderer: Renderer) -> Result<(), String> {
        if self.stream.is_some() {
            return Err("audio stream already initialized".to_owned());
        }
        let producer = if self.capture_enabled {
            let (producer, consumer) = RingBuffer::new(CAPTURE_FRAMES);
            self.capture = Some(consumer);
            Some(producer)
        } else {
            None
        };
        let callback = Box::new(OutputCallback {
            renderer,
            scratch: vec![0.0; CALLBACK_SAMPLES].into_boxed_slice(),
            counters: Arc::clone(&self.counters),
            capture: producer,
        });
        let spec = audio::SDL_AudioSpec {
            format: audio::SDL_AUDIO_F32,
            channels: 2,
            freq: OUTPUT_RATE_I32,
        };
        // Hand stable ownership to the raw-pointer owner before SDL can call
        // it. No subsequent Box moves or application references alias callback.
        let callback_raw = Box::into_raw(callback);
        let userdata = callback_raw.cast::<c_void>();
        // SAFETY: SDL is initialized by the retained AudioSubsystem. spec lives
        // through this synchronous open; boxed callback userdata stays stable
        // until Drop destroys the stream and synchronizes its callback first.
        let raw = unsafe {
            audio::SDL_OpenAudioDeviceStream(
                audio::SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,
                &raw const spec,
                Some(render_callback),
                userdata,
            )
        };
        let Some(stream) = NonNull::new(raw) else {
            // SAFETY: open failed, so SDL retained no callback. callback_raw
            // came from Box::into_raw above and has not been reconstructed.
            unsafe {
                drop(Box::from_raw(callback_raw));
            }
            return Err(sdl3::get_error().to_string());
        };
        self.callback = NonNull::new(callback_raw);
        self.stream = Some(stream);
        // SAFETY: stream is an owned, live SDL audio stream from the call above.
        if !unsafe { audio::SDL_ResumeAudioStreamDevice(stream.as_ptr()) } {
            return Err(sdl3::get_error().to_string());
        }
        Ok(())
    }
}

impl Drop for SdlBackend {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.take() {
            // SAFETY: this uniquely owned stream is destroyed exactly once;
            // SDL unbinds/closes its device and finishes callbacks before our
            // callback Box and AudioSubsystem fields are dropped afterward.
            unsafe {
                audio::SDL_DestroyAudioStream(stream.as_ptr());
            }
        }
        if let Some(callback) = self.callback.take() {
            // SAFETY: the stream and all callbacks have finished above. This
            // pointer uniquely owns the original Box::into_raw allocation.
            unsafe {
                drop(Box::from_raw(callback.as_ptr()));
            }
        }
    }
}

unsafe extern "C" fn render_callback(
    userdata: *mut c_void,
    stream: *mut SDL_AudioStream,
    additional_bytes: c_int,
    _total_bytes: c_int,
) {
    if userdata.is_null() || stream.is_null() || additional_bytes <= 0_i32 {
        return;
    }
    // SAFETY: SDL passes the stable boxed OutputCallback pointer supplied at
    // open. Its per-stream lock serializes callbacks; the owner destroys the
    // stream before freeing this Box and never otherwise accesses its fields.
    let output = unsafe { &mut *userdata.cast::<OutputCallback>() };
    let requested = usize::try_from(additional_bytes)
        .unwrap_or_default()
        .checked_div(size_of::<f32>())
        .unwrap_or_default();
    let mut remaining = requested.min(MAX_CALLBACK_SAMPLES);
    remaining = remaining.saturating_sub(remaining % 2);
    resource_boundary(&output.counters.resource_gate, || {
        output.renderer.on_start_processing();
    });
    while remaining > 0 {
        let count = remaining.min(output.scratch.len());
        let Some(samples) = output.scratch.get_mut(..count) else {
            break;
        };
        output.renderer.process(samples, 2);
        let mut peak = 0.0_f32;
        for sample in samples.iter_mut() {
            if !sample.is_finite() {
                *sample = 0.0;
            }
            peak = peak.max(sample.abs());
        }
        let _previous_peak = output
            .counters
            .peak
            .fetch_max(peak.to_bits(), Ordering::Relaxed);
        if let Some(capture) = &mut output.capture {
            for pair in samples.as_chunks::<2>().0 {
                if capture.push(*pair).is_err() {
                    let _dropped = output
                        .counters
                        .capture_dropped
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        let byte_count =
            c_int::try_from(count.saturating_mul(size_of::<f32>())).unwrap_or_default();
        // SAFETY: samples is a valid contiguous f32 buffer of byte_count bytes;
        // SDL copies it synchronously while its callback holds the stream lock.
        let submitted =
            unsafe { audio::SDL_PutAudioStreamData(stream, samples.as_ptr().cast(), byte_count) };
        if !submitted {
            let _errors = output.counters.errors.fetch_add(1, Ordering::Relaxed);
            break;
        }
        let _frames = output.counters.frames.fetch_add(
            u64::try_from(count / 2).unwrap_or_default(),
            Ordering::Relaxed,
        );
        remaining = remaining.saturating_sub(count);
    }
}

struct LoadedCue {
    id: String,
    variants: Vec<StaticSoundData>,
    next_variant: usize,
    gain: f32,
    looping: bool,
    spatial: bool,
    min_distance: f32,
    max_distance: f32,
}

struct Voice {
    handle: StaticSoundHandle,
    source: Option<SourceId>,
    position: Option<Vec3>,
    gain: f32,
    time_domain: TimeDomain,
    bus: AudioBus,
    spatial: bool,
    min_distance: f32,
    max_distance: f32,
    paused: bool,
    cue_index: usize,
    looping: bool,
}

#[derive(Clone, Copy)]
struct PendingPlay {
    cue_index: usize,
    request: PlayRequest,
}

#[derive(Clone, Copy)]
struct Mix {
    listener: Listener,
    right: Vec3,
    settings: AudioSettings,
    paused: bool,
    world_scale: f32,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            listener: Listener::default(),
            right: Vec3::X,
            settings: AudioSettings::default(),
            paused: false,
            world_scale: 1.0,
        }
    }
}

pub(super) struct NativeAudio {
    manager: AudioManager<SdlBackend>,
    cues: Vec<LoadedCue>,
    voices: Vec<Option<Voice>>,
    pending: Vec<Option<PendingPlay>>,
    mix: Mix,
    stats: AudioStats,
    counters: Arc<OutputCounters>,
}

impl NativeAudio {
    pub fn load(asset_root: &Path, sdl: &sdl3::Sdl) -> Result<Self, String> {
        let catalog = AudioCatalog::load(asset_root)?;
        let (cues, loaded_clips, decoded_bytes) = load_clips(asset_root, catalog)?;
        let subsystem = sdl.audio().map_err(|error| error.to_string())?;
        let mut manager = AudioManager::<SdlBackend>::new(AudioManagerSettings {
            capacities: Capacities {
                sub_track_capacity: 0,
                send_track_capacity: 0,
                clock_capacity: 0,
                modulator_capacity: 0,
                listener_capacity: 0,
            },
            main_track_builder: MainTrackBuilder::new().sound_capacity(MAX_VOICES),
            internal_buffer_size: 128,
            backend_settings: SdlSettings {
                subsystem,
                capture: std::env::var("PLACES_AUDIO_CAPTURE").is_ok_and(|value| value == "1"),
            },
        })?;
        let counters = Arc::clone(&manager.backend_mut().counters);
        Ok(Self {
            manager,
            cues,
            voices: std::iter::repeat_with(|| None).take(MAX_VOICES).collect(),
            pending: vec![None; MAX_VOICES],
            mix: Mix::default(),
            stats: AudioStats {
                enabled: true,
                loaded_clips,
                decoded_bytes,
                ..AudioStats::default()
            },
            counters,
        })
    }

    pub fn play(&mut self, id: &str, request: PlayRequest) -> bool {
        if !valid_gain(request.gain)
            || request
                .position
                .is_some_and(|position| !valid_position(position))
        {
            return self.reject();
        }
        self.reap();
        let Some(cue_index) = self.cues.iter().position(|cue| cue.id == id) else {
            return self.reject();
        };
        if let Some(source) = request.source {
            self.stop_source(source);
        }
        if self
            .voices
            .iter()
            .flatten()
            .count()
            .saturating_add(self.pending.iter().flatten().count())
            >= MAX_VOICES
        {
            return self.reject();
        }
        if self.start(cue_index, request) {
            return true;
        }
        // A source-owned loop must survive replacement while its old voice is
        // asynchronously retiring. Short one-shot overflow is deliberately dropped.
        let looping = request
            .looping
            .unwrap_or_else(|| self.cues.get(cue_index).is_some_and(|cue| cue.looping));
        if looping
            && request.source.is_some()
            && let Some(slot) = self.pending.iter_mut().find(|slot| slot.is_none())
        {
            *slot = Some(PendingPlay { cue_index, request });
            return true;
        }
        self.reject()
    }

    fn start(&mut self, cue_index: usize, request: PlayRequest) -> bool {
        let Some(_resource_guard) = ResourceGate::try_acquire(&self.counters.resource_gate) else {
            return false;
        };
        // Stopped voices remain in Kira until the next device callback. Check
        // its real bounded occupancy before creating even small voice controls.
        if self.manager.main_track().num_sounds() >= MAX_VOICES {
            return false;
        }
        let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };
        let Some(cue) = self.cues.get_mut(cue_index) else {
            return false;
        };
        let Some(data) = cue.variants.get(cue.next_variant) else {
            return false;
        };
        let mut sound = data.clone();
        let looping = request.looping.unwrap_or(cue.looping);
        if looping {
            sound = sound.loop_region(..).fade_in_tween(short_tween());
        }
        let (amplitude, pan) = spatial_mix(
            self.mix,
            request.position,
            request.spatial.unwrap_or(cue.spatial),
            cue.min_distance,
            cue.max_distance,
            request.gain * cue.gain,
            request.bus,
        );
        let paused = world_paused(self.mix, request.time_domain);
        sound = sound
            .volume(decibels(if paused { 0.0 } else { amplitude }))
            .panning(pan)
            .playback_rate(playback_rate(self.mix, request.time_domain));
        match self.manager.play(sound) {
            Ok(mut handle) => {
                if paused {
                    handle.pause(immediate());
                }
                *slot = Some(Voice {
                    handle,
                    source: request.source,
                    position: request.position,
                    gain: request.gain * cue.gain,
                    time_domain: request.time_domain,
                    bus: request.bus,
                    spatial: request.spatial.unwrap_or(cue.spatial),
                    min_distance: cue.min_distance,
                    max_distance: cue.max_distance,
                    paused,
                    cue_index,
                    looping,
                });
                cue.next_variant = cue
                    .next_variant
                    .saturating_add(1)
                    .checked_rem(cue.variants.len())
                    .unwrap_or_default();
                self.stats.played = self.stats.played.saturating_add(1);
                true
            }
            Err(_capacity_error) => {
                self.stats.errors = self.stats.errors.saturating_add(1);
                false
            }
        }
    }

    const fn reject(&mut self) -> bool {
        self.stats.dropped = self.stats.dropped.saturating_add(1);
        false
    }

    fn reap(&mut self) {
        for slot in &mut self.voices {
            if slot
                .as_ref()
                .is_some_and(|voice| voice.handle.state() == PlaybackState::Stopped)
            {
                *slot = None;
            }
        }
    }

    pub fn update(
        &mut self,
        listener: Listener,
        paused: bool,
        scale: f32,
        settings: AudioSettings,
    ) {
        if let Some((validated_listener, right)) = normalize_listener(listener) {
            self.mix.listener = validated_listener;
            self.mix.right = right;
        }
        self.mix.paused = paused;
        self.mix.world_scale = sanitize_world_scale(scale);
        self.mix.settings = AudioSettings {
            master_volume: sanitize_gain(settings.master_volume),
            effects_volume: sanitize_gain(settings.effects_volume),
            ambient_volume: sanitize_gain(settings.ambient_volume),
            muted: settings.muted,
        };
        self.reap();
        self.retry_pending();
        for voice in self.voices.iter_mut().flatten() {
            update_voice(voice, self.mix);
        }
    }

    pub fn set_source_position(&mut self, source: SourceId, position: Vec3) {
        if !valid_position(position) {
            return;
        }
        for voice in self
            .voices
            .iter_mut()
            .flatten()
            .filter(|voice| voice.source == Some(source))
        {
            voice.position = Some(position);
            update_voice(voice, self.mix);
        }
        for pending in self
            .pending
            .iter_mut()
            .flatten()
            .filter(|pending| pending.request.source == Some(source))
        {
            pending.request.position = Some(position);
        }
    }

    pub fn stop_source(&mut self, source: SourceId) {
        for pending in &mut self.pending {
            if pending
                .as_ref()
                .is_some_and(|play| play.request.source == Some(source))
            {
                *pending = None;
            }
        }
        for slot in &mut self.voices {
            if slot
                .as_ref()
                .is_some_and(|voice| voice.source == Some(source))
            {
                if let Some(voice) = slot {
                    voice.handle.stop(if voice.looping {
                        short_tween()
                    } else {
                        immediate()
                    });
                }
                *slot = None;
            }
        }
    }

    pub fn update_source_positions(&mut self, resolve: &mut impl FnMut(SourceId) -> Option<Vec3>) {
        for slot in &mut self.voices {
            let Some(voice) = slot else {
                continue;
            };
            let Some(source) = voice.source else {
                continue;
            };
            if let Some(position) = resolve(source).filter(|position| valid_position(*position)) {
                voice.position = Some(position);
                update_voice(voice, self.mix);
            } else {
                voice.handle.stop(immediate());
                *slot = None;
            }
        }
        for slot in &mut self.pending {
            let Some(pending) = slot else {
                continue;
            };
            let Some(source) = pending.request.source else {
                continue;
            };
            if let Some(position) = resolve(source).filter(|point| valid_position(*point)) {
                pending.request.position = Some(position);
            } else {
                *slot = None;
            }
        }
    }

    pub fn clear_world(&mut self) {
        self.pending.fill(None);
        for slot in &mut self.voices {
            if let Some(voice) = slot {
                voice.handle.stop(immediate());
            }
            *slot = None;
        }
    }

    fn retry_pending(&mut self) {
        for index in 0..self.pending.len() {
            let Some(request) = self.pending.get_mut(index).and_then(Option::take) else {
                continue;
            };
            if !self.start(request.cue_index, request.request)
                && let Some(slot) = self.pending.get_mut(index)
            {
                *slot = Some(request);
            }
        }
    }

    pub fn stats(&self) -> AudioStats {
        AudioStats {
            active_voices: self
                .voices
                .iter()
                .flatten()
                .filter(|voice| voice.handle.state() != PlaybackState::Stopped)
                .count(),
            pending_voices: self.pending.iter().flatten().count(),
            errors: self
                .stats
                .errors
                .saturating_add(self.counters.errors.load(Ordering::Relaxed)),
            output_frames: self.counters.frames.load(Ordering::Relaxed),
            peak_amplitude: f32::from_bits(self.counters.peak.load(Ordering::Relaxed)),
            capture_dropped: self.counters.capture_dropped.load(Ordering::Relaxed),
            ..self.stats
        }
    }

    pub fn voice_snapshot(&self) -> Vec<AudioVoiceSnapshot> {
        let mut snapshots = Vec::with_capacity(MAX_VOICES);
        for voice in self
            .voices
            .iter()
            .flatten()
            .filter(|voice| voice.handle.state() != PlaybackState::Stopped)
        {
            if let Some(cue) = self.cues.get(voice.cue_index) {
                let (gain, pan) = spatial_mix(
                    self.mix,
                    voice.position,
                    voice.spatial,
                    voice.min_distance,
                    voice.max_distance,
                    voice.gain,
                    voice.bus,
                );
                snapshots.push(AudioVoiceSnapshot {
                    cue: cue.id.clone(),
                    source: voice.source.map(|source| source.0),
                    position: voice.position.as_ref().map(Vec3::to_array),
                    spatial: voice.spatial,
                    gain: if world_paused(self.mix, voice.time_domain) {
                        0.0
                    } else {
                        gain
                    },
                    pan,
                    playback_rate: playback_rate(self.mix, voice.time_domain),
                    looping: voice.looping,
                    time_domain: voice.time_domain,
                    paused: world_paused(self.mix, voice.time_domain),
                    pending: false,
                });
            }
        }
        for play in self.pending.iter().flatten() {
            if let Some(cue) = self.cues.get(play.cue_index) {
                let request = play.request;
                let (gain, pan) = spatial_mix(
                    self.mix,
                    request.position,
                    request.spatial.unwrap_or(cue.spatial),
                    cue.min_distance,
                    cue.max_distance,
                    request.gain * cue.gain,
                    request.bus,
                );
                snapshots.push(AudioVoiceSnapshot {
                    cue: cue.id.clone(),
                    source: request.source.map(|source| source.0),
                    position: request.position.as_ref().map(Vec3::to_array),
                    spatial: request.spatial.unwrap_or(cue.spatial),
                    gain: if world_paused(self.mix, request.time_domain) {
                        0.0
                    } else {
                        gain
                    },
                    pan,
                    playback_rate: playback_rate(self.mix, request.time_domain),
                    looping: request.looping.unwrap_or(cue.looping),
                    time_domain: request.time_domain,
                    paused: world_paused(self.mix, request.time_domain),
                    pending: true,
                });
            }
        }
        snapshots
    }

    pub fn drain_capture(&mut self, output: &mut [[f32; 2]]) -> usize {
        let Some(capture) = &mut self.manager.backend_mut().capture else {
            return 0;
        };
        let mut written = 0_usize;
        for slot in output {
            let Ok(frame) = capture.pop() else {
                break;
            };
            *slot = frame;
            written = written.saturating_add(1);
        }
        written
    }
}

fn update_voice(voice: &mut Voice, mix: Mix) {
    let paused = world_paused(mix, voice.time_domain);
    let (amplitude, pan) = spatial_mix(
        mix,
        voice.position,
        voice.spatial,
        voice.min_distance,
        voice.max_distance,
        voice.gain,
        voice.bus,
    );
    voice.handle.set_volume(
        decibels(if paused { 0.0 } else { amplitude }),
        short_tween(),
    );
    voice.handle.set_panning(pan, short_tween());
    voice
        .handle
        .set_playback_rate(playback_rate(mix, voice.time_domain), short_tween());
    // Kira's pause/resume writers are separate latest-value slots. Reconcile
    // with the acknowledged state so a rapid pause/resume/pause cannot leave
    // a stale resume active after the application has requested pause again.
    if paused {
        voice.handle.pause(immediate());
    } else if voice.paused
        || matches!(
            voice.handle.state(),
            PlaybackState::Paused | PlaybackState::Pausing | PlaybackState::WaitingToResume
        )
    {
        voice.handle.resume(immediate());
    }
    voice.paused = paused;
}

fn world_paused(mix: Mix, domain: TimeDomain) -> bool {
    mix.paused || (domain == TimeDomain::World && mix.world_scale <= 0.0)
}
fn playback_rate(mix: Mix, domain: TimeDomain) -> f64 {
    match domain {
        TimeDomain::World => f64::from(mix.world_scale),
        TimeDomain::Real => 1.0,
    }
}
const fn sanitize_gain(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
const fn sanitize_world_scale(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 4.0)
    } else {
        0.0
    }
}
fn normalize_listener(listener: Listener) -> Option<(Listener, Vec3)> {
    if !valid_position(listener.position)
        || !listener.forward.is_finite()
        || !listener.up.is_finite()
    {
        return None;
    }
    let forward = listener.forward.try_normalize()?;
    let up = listener.up.try_normalize()?;
    let right = forward.cross(up).try_normalize()?;
    Some((
        Listener {
            position: listener.position,
            forward,
            up,
        },
        right,
    ))
}
fn valid_position(position: Vec3) -> bool {
    position.is_finite() && position.abs().max_element() <= 1_000_000.0
}
fn decibels(amplitude: f32) -> Decibels {
    if amplitude <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amplitude.log10())
    }
}
fn immediate() -> Tween {
    Tween {
        duration: Duration::ZERO,
        ..Tween::default()
    }
}
fn short_tween() -> Tween {
    Tween {
        duration: Duration::from_millis(12),
        ..Tween::default()
    }
}

fn spatial_mix(
    mix: Mix,
    position: Option<Vec3>,
    spatial: bool,
    near: f32,
    far: f32,
    gain: f32,
    bus: AudioBus,
) -> (f32, f32) {
    let bus_gain = match bus {
        AudioBus::Effects => mix.settings.effects_volume,
        AudioBus::Ambient => mix.settings.ambient_volume,
    };
    let mut amplitude = if mix.settings.muted {
        0.0
    } else {
        gain * mix.settings.master_volume * bus_gain
    };
    let mut pan = 0.0_f32;
    if spatial && let Some(emitter_position) = position {
        // glam component subtraction is finite bounded floating point math.
        #[expect(
            clippy::arithmetic_side_effects,
            reason = "validated world positions use finite bounded floating point components, so vector subtraction cannot overflow integers or panic"
        )]
        let offset = emitter_position - mix.listener.position;
        let distance = offset.length();
        if distance > near {
            let fade = ((far - distance) / (far - near)).clamp(0.0, 1.0);
            amplitude *= fade * fade;
        }
        if let Some(direction) = offset.try_normalize() {
            pan = direction.dot(mix.right).clamp(-1.0, 1.0);
        }
    }
    (amplitude, pan)
}

fn load_clips(
    asset_root: &Path,
    catalog: AudioCatalog,
) -> Result<(Vec<LoadedCue>, usize, usize), String> {
    let root = fs::canonicalize(asset_root.join("audio")).map_err(|error| error.to_string())?;
    let mut cache: Vec<(PathBuf, StaticSoundData, u16)> = Vec::new();
    let mut decoded_bytes = 0_usize;
    let mut cues = Vec::with_capacity(catalog.cues.len());
    for cue in catalog.cues {
        let mut variants = Vec::with_capacity(cue.files.len());
        for file in &cue.files {
            let path =
                fs::canonicalize(root.join(file)).map_err(|error| format!("{file}: {error}"))?;
            if !path.starts_with(&root) {
                return Err(format!("audio file escapes asset root: {file}"));
            }
            if let Some((_, data, channels)) = cache.iter().find(|(cached, _, _)| *cached == path) {
                if cue.spatial && *channels != 1 {
                    return Err(format!("spatial audio requires a mono WAV: {file}"));
                }
                variants.push(data.clone());
                continue;
            }
            let bytes = read_bounded(&path)?;
            let info = wav_info(&bytes, cue.spatial)?;
            let predicted = info.frames.saturating_mul(size_of::<kira::Frame>());
            if decoded_bytes.saturating_add(predicted) > MAX_DECODED_BYTES {
                return Err("audio library exceeds 64 MiB decoded budget".to_owned());
            }
            let data = StaticSoundData::from_cursor(Cursor::new(bytes))
                .map_err(|error| format!("{file}: {error}"))?;
            if data.frames.len() != info.frames
                || data.sample_rate != info.sample_rate
                || data
                    .frames
                    .iter()
                    .any(|frame| !frame.left.is_finite() || !frame.right.is_finite())
            {
                return Err(format!(
                    "decoded WAV does not match validated header: {file}"
                ));
            }
            decoded_bytes = decoded_bytes.saturating_add(predicted);
            variants.push(data.clone());
            cache.push((path, data, info.channels));
        }
        cues.push(LoadedCue {
            id: cue.id,
            variants,
            next_variant: 0,
            gain: cue.gain,
            looping: cue.looping,
            spatial: cue.spatial,
            min_distance: cue.min_distance,
            max_distance: cue.max_distance,
        });
    }
    Ok((cues, cache.len(), decoded_bytes))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let _read = file
        .take(MAX_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
        return Err("WAV exceeds 16 MiB file limit".to_owned());
    }
    Ok(bytes)
}

struct WavInfo {
    frames: usize,
    sample_rate: u32,
    channels: u16,
}

/// Validate PCM16 headers and exact decoded size before invoking the decoder.
fn wav_info(bytes: &[u8], spatial: bool) -> Result<WavInfo, String> {
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("audio files must be RIFF WAV".to_owned());
    }
    let declared_bytes = usize::try_from(little_u32(bytes.get(4..8))?)
        .unwrap_or(usize::MAX)
        .saturating_add(8);
    if declared_bytes != bytes.len() {
        return Err("WAV RIFF length does not match file".to_owned());
    }
    let mut offset = 12_usize;
    let mut format = None;
    let mut data_bytes = None;
    while let Some(chunk) = bytes.get(offset..offset.saturating_add(8)) {
        let size = little_u32(chunk.get(4..8))?;
        let start = offset.saturating_add(8);
        let end = start.saturating_add(usize::try_from(size).unwrap_or(usize::MAX));
        let payload = bytes
            .get(start..end)
            .ok_or_else(|| "truncated WAV chunk".to_owned())?;
        match chunk.get(..4) {
            Some(b"fmt ") => {
                if format.is_some() {
                    return Err("duplicate WAV format".to_owned());
                }
                format = Some((
                    little_u16(payload.get(..2))?,
                    little_u16(payload.get(2..4))?,
                    little_u32(payload.get(4..8))?,
                    little_u16(payload.get(12..14))?,
                    little_u16(payload.get(14..16))?,
                ));
            }
            Some(b"data") if data_bytes.replace(size).is_some() => {
                return Err("duplicate WAV data".to_owned());
            }
            _ => {}
        }
        offset = end.saturating_add(usize::try_from(size % 2).unwrap_or_default());
    }
    let (encoding, channels, sample_rate, block, bits) =
        format.ok_or_else(|| "WAV lacks format".to_owned())?;
    let size = data_bytes.ok_or_else(|| "WAV lacks sample data".to_owned())?;
    if encoding != 1
        || bits != 16
        || !(1..=2).contains(&channels)
        || (spatial && channels != 1)
        || !(8_000..=48_000).contains(&sample_rate)
        || block != channels.saturating_mul(2)
        || size == 0
        || size.checked_rem(u32::from(block)) != Some(0)
    {
        return Err(
            "WAV requires PCM16, spatial mono/ambient mono or stereo, and 8–48 kHz".to_owned(),
        );
    }
    let frames = size.checked_div(u32::from(block)).unwrap_or_default();
    if frames > sample_rate.saturating_mul(MAX_CLIP_SECONDS) {
        return Err("WAV exceeds 30 second clip limit".to_owned());
    }
    Ok(WavInfo {
        frames: usize::try_from(frames).map_err(|error| error.to_string())?,
        sample_rate,
        channels,
    })
}

fn little_u16(bytes: Option<&[u8]>) -> Result<u16, String> {
    let array = bytes
        .and_then(|value| <[u8; 2]>::try_from(value).ok())
        .ok_or_else(|| "truncated WAV integer".to_owned())?;
    Ok(u16::from_le_bytes(array))
}
fn little_u32(bytes: Option<&[u8]>) -> Result<u32, String> {
    let array = bytes
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .ok_or_else(|| "truncated WAV integer".to_owned())?;
    Ok(u32::from_le_bytes(array))
}

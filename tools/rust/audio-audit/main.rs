//! Explicit native device/allocator/lifetime audit; never part of CI or tests.
use glam::Vec3;
use places::engine::audio::acoustics::AcousticScene;
use places::engine::audio::{
    AudioSettings, AudioSystem, Listener, PlayRequest, SourceId, SpatialMode,
};
use places::engine::level::LevelDef;
use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct AuditAllocator;
static CALLBACK_ALLOCS: AtomicU64 = AtomicU64::new(0);
static CALLBACK_FREES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_BYTES: AtomicU64 = AtomicU64::new(0);
#[global_allocator]
static ALLOCATOR: AuditAllocator = AuditAllocator;
// SAFETY: every allocation operation forwards the identical pointer/layout to
// System; the extra TLS/atomic counters never allocate or change ownership.
unsafe impl GlobalAlloc for AuditAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if places::engine::audio::realtime_active() {
            let _count = CALLBACK_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: GlobalAlloc caller supplies a valid layout; System owns the result.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if places::engine::audio::realtime_active() {
            let _count = CALLBACK_FREES.fetch_add(1, Ordering::Relaxed);
        }
        released(layout.size());
        // SAFETY: pointer/layout retain their original GlobalAlloc ownership.
        unsafe {
            System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if places::engine::audio::realtime_active() {
            let _count = CALLBACK_ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: original layout and requested size come unchanged from allocator caller.
        let pointer = unsafe { System.realloc(ptr, layout, size) };
        if !pointer.is_null() {
            if size >= layout.size() {
                allocated(size.saturating_sub(layout.size()));
            } else {
                released(layout.size().saturating_sub(size));
            }
        }
        pointer
    }
}
fn fixture(material: &str) -> Result<LevelDef, String> {
    LevelDef::from_json(&serde_json::json!({"schema":"places-level","format_version":1,"id":"audio_audit","name":"Audio audit","spawn":{"x":2,"z":2},"defaults":{"floor":material,"wall":material,"ceiling":material},"rooms":[{"x":0,"z":0,"width":8,"depth":10,"height":3}],"walls":[{"x":0,"z":5,"width":8,"depth":0.2,"height":3,"openings":[{"offset":3,"width":2,"height":2.5}]}],"doors":[{"id":"door","x":3,"z":5.1,"width":2,"height":2.5}]}).to_string()).map_err(|error|error.to_string())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let assets = PathBuf::from(
        args.next()
            .ok_or("Usage: places-audio-audit <assets> <new-output-directory>")?,
    );
    let out = PathBuf::from(args.next().ok_or("Missing output directory")?);
    if args.next().is_some() || out.exists() {
        return Err("Use a new output directory and exactly two arguments".into());
    }
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let heap_before_audio = LIVE_BYTES.load(Ordering::Relaxed);
    let mut audio = AudioSystem::new(&assets, &sdl);
    let heap_after_audio = LIVE_BYTES.load(Ordering::Relaxed);
    if !audio.stats().enabled {
        return Err("Native output unavailable; inspect actual device log".into());
    }
    let mut results = Vec::new();
    let started = Instant::now();
    let mut settings = AudioSettings::default();
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    let mut heap_samples = Vec::with_capacity(32);
    for cycle in 0_u16..32 {
        run_cycle(
            &mut audio,
            &mut settings,
            &mut scratch,
            &mut results,
            &out,
            cycle,
        )?;
        heap_samples.push(serde_json::json!({"cycle":cycle,"live_rust_bytes":LIVE_BYTES.load(Ordering::Relaxed),"peak_rust_bytes":PEAK_BYTES.load(Ordering::Relaxed)}));
    }
    measure_latency(&mut audio, &out)?;
    audio.unload();
    let heap_after_unload = LIVE_BYTES.load(Ordering::Relaxed);
    std::fs::write(
        out.join("heap-samples.json"),
        serde_json::to_vec_pretty(&heap_samples).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let cycles = results.len();
    drop(results);
    drop(heap_samples);
    drop(scratch);
    let heap_after_harness_cleanup = LIVE_BYTES.load(Ordering::Relaxed);
    let summary = serde_json::json!({"heap_after_harness_cleanup":heap_after_harness_cleanup,"seconds":started.elapsed().as_secs_f64(),"cycles":cycles,"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"unloaded":audio.stats(),"heap_before_audio":heap_before_audio,"heap_after_audio":heap_after_audio,"heap_after_unload":heap_after_unload,"peak_rust_bytes":PEAK_BYTES.load(Ordering::Relaxed),"subjective_listening":false});
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if CALLBACK_ALLOCS.load(Ordering::Relaxed) > 0 || CALLBACK_FREES.load(Ordering::Relaxed) > 0 {
        return Err("Real-time allocator activity detected; inspect results".into());
    }
    Ok(())
}
fn write_wav(path: &std::path::Path, pcm: &[[f32; 2]]) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let bytes = u32::try_from(pcm.len().saturating_mul(8)).map_err(|error| error.to_string())?;
    for data in [
        b"RIFF".as_slice(),
        &bytes.saturating_add(36).to_le_bytes(),
        b"WAVEfmt ",
        &16_u32.to_le_bytes(),
        &3_u16.to_le_bytes(),
        &2_u16.to_le_bytes(),
        &48_000_u32.to_le_bytes(),
        &384_000_u32.to_le_bytes(),
        &8_u16.to_le_bytes(),
        &32_u16.to_le_bytes(),
        b"data",
        &bytes.to_le_bytes(),
    ] {
        file.write_all(data).map_err(|error| error.to_string())?;
    }
    for pair in pcm {
        for sample in pair {
            file.write_all(&sample.to_le_bytes())
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn run_cycle(
    audio: &mut AudioSystem,
    settings: &mut AudioSettings,
    scratch: &mut [[f32; 2]],
    results: &mut Vec<serde_json::Value>,
    out: &std::path::Path,
    cycle: u16,
) -> Result<(), String> {
    let tile = cycle % 2 == 1;
    let level = fixture(if tile {
        "core:pool_tile_deck_01"
    } else {
        "core:carpet_beige_01"
    })?;
    audio.clear_world();
    audio.set_acoustic_scene(AcousticScene::build(&level));
    settings.spatial_mode = if cycle % 4 >= 2 {
        SpatialMode::Headphones
    } else {
        SpatialMode::Speakers
    };
    settings.muted = false;
    let listener = Listener {
        position: Vec3::new(4.0, 1.6, 3.0),
        ..Listener::default()
    };
    audio.update(listener, false, 1.0, *settings);
    let count = if cycle >= 8 { 48_u16 } else { 3 };
    let before = audio.stats();
    let mut accepted = 0_u16;
    for source in 0..count {
        accepted = accepted.saturating_add(u16::from(audio.play(
            "fluorescent_buzz",
            PlayRequest {
                source: Some(SourceId(u64::from(source))),
                position: Some(Vec3::new(4.0, 2.0, 7.0)),
                looping: Some(true),
                // Stay above Kira's -60 dB digital-silence floor after distance/bus gain.
                gain: 0.5,
                ..PlayRequest::default()
            },
        )));
    }
    // Deliberate overflow must refuse capacity without allocating callback work.
    let overflow = audio.play("switch_click", PlayRequest::default());
    let duration = Instant::now();
    let mut pcm = Vec::<[f32; 2]>::new();
    let mut snapshots = Vec::new();
    let (signal_frames, pcm_frames) = {
        let mut capture = IntervalCapture {
            pcm: &mut pcm,
            snapshots: &mut snapshots,
            cycle,
            listener,
            started: duration,
            signal_frames: 0,
            pcm_frames: 0,
        };
        pump_interval(audio, settings, scratch, &mut capture)?;
        (capture.signal_frames, capture.pcm_frames)
    };
    if signal_frames == 0 {
        return Err(
            "audit produced no nonzero native PCM; silent voices are not FIR evidence".into(),
        );
    }
    let active = audio.stats();
    audio.clear_world();
    for _ in 0_u16..12 {
        audio.update(listener, false, 1.0, *settings);
        let _drained = audio.drain_capture(scratch);
        std::thread::sleep(Duration::from_millis(5));
    }
    let cleared = audio.stats();
    if cycle < 12 {
        write_wav(&out.join(format!("cycle-{cycle}.wav")), &pcm)?;
    }
    let snapshots_name = format!("cycle-{cycle}.json");
    std::fs::write(
        out.join(&snapshots_name),
        serde_json::to_vec_pretty(&snapshots).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let row = serde_json::json!({"signal_frames":signal_frames,"pcm_frames":pcm_frames,"cycle":cycle,"tile":tile,"mode":format!("{:?}",settings.spatial_mode),"requested_voices":count,"accepted":accepted,"overflow_accepted":overflow,"duration_seconds":duration.elapsed().as_secs_f64(),"before":before,"active":active,"cleared":cleared,"snapshots_file":snapshots_name});
    results.push(row);
    std::fs::write(
        out.join("results.json"),
        serde_json::to_vec_pretty(&results).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if cleared.active_voices != 0 || cleared.pending_voices != 0 || cleared.acoustic_surfaces != 0 {
        return Err("Old-world audio ownership retained after clear".into());
    }
    places::engine::logging::info(format!(
        "[audio-audit] completed cycle {cycle} in {:.2}s",
        duration.elapsed().as_secs_f64()
    ));
    Ok(())
}

struct IntervalCapture<'a> {
    pcm: &'a mut Vec<[f32; 2]>,
    snapshots: &'a mut Vec<serde_json::Value>,
    cycle: u16,
    listener: Listener,
    started: Instant,
    signal_frames: usize,
    pcm_frames: usize,
}

fn pump_interval(
    audio: &mut AudioSystem,
    settings: &mut AudioSettings,
    scratch: &mut [[f32; 2]],
    capture: &mut IntervalCapture<'_>,
) -> Result<(), String> {
    for step in 0_u16..400 {
        capture.listener.forward = if step % 200 < 100 {
            Vec3::NEG_Z
        } else {
            Vec3::X
        };
        if step % 100 < 50 {
            audio.set_source_position(SourceId(0), Vec3::new(4.0, 2.0, 7.0));
        } else {
            audio.set_source_position(SourceId(0), Vec3::new(7.0, 0.5, 2.0));
        }
        settings.muted = (250..275).contains(&step);
        let paused = (300..325).contains(&step);
        audio.update(capture.listener, paused, 1.0, *settings);
        let captured = audio.drain_capture(scratch);
        capture.pcm_frames = capture.pcm_frames.saturating_add(captured);
        capture.signal_frames = capture.signal_frames.saturating_add(
            scratch
                .get(..captured)
                .unwrap_or_default()
                .iter()
                .filter(|pair| pair.iter().any(|sample| sample.abs() > 1.0e-9))
                .count(),
        );
        if capture.cycle < 12 {
            capture
                .pcm
                .extend_from_slice(scratch.get(..captured).unwrap_or_default());
        }
        if step % 50 == 0 {
            let voices = audio.voice_snapshot();
            if !settings.muted && !paused {
                let expected = if capture.cycle >= 8 { 48_usize } else { 3 };
                let audible = voices
                    .iter()
                    .filter(|voice| {
                        voice.cue == "fluorescent_buzz"
                            && !voice.pending
                            && !voice.paused
                            && voice.gain > 0.001_f32
                    })
                    .count();
                if audible != expected {
                    return Err("audit voices must all exceed Kira's digital-silence floor".into());
                }
            }
            capture.snapshots.push(serde_json::json!({"step":step,"seconds":capture.started.elapsed().as_secs_f64(),"stats":audio.stats(),"voices":voices}));
        }
        if step % 40 == 0 {
            let _restarted = audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(0)),
                    position: Some(Vec3::new(4.0, 2.0, 7.0)),
                    looping: Some(true),
                    gain: 0.5,
                    ..PlayRequest::default()
                },
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn allocated(bytes: usize) {
    let amount = u64::try_from(bytes).unwrap_or(u64::MAX);
    let before = LIVE_BYTES.fetch_add(amount, Ordering::Relaxed);
    let _peak = PEAK_BYTES.fetch_max(before.saturating_add(amount), Ordering::Relaxed);
}
fn released(bytes: usize) {
    let _before = LIVE_BYTES.fetch_sub(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::Relaxed);
}

// Measures request-to-native post-mixer tap response. It excludes the host/DAC
// transport and human perception; mute includes a 10 ms exact-silence witness.
fn measure_latency(audio: &mut AudioSystem, out: &std::path::Path) -> Result<(), String> {
    let mut results = Vec::new();
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    for mode in [SpatialMode::Speakers, SpatialMode::Headphones] {
        let mut settings = AudioSettings {
            spatial_mode: mode,
            ..AudioSettings::default()
        };
        for trial in 0_u8..3 {
            settings.muted = false;
            audio.clear_world();
            audio.set_acoustic_scene(AcousticScene::build(&fixture("core:pool_tile_deck_01")?));
            let _quiet = drain_for(audio, settings, &mut scratch, 60);
            let started = Instant::now();
            if !audio.play(
                "switch_click",
                PlayRequest {
                    position: Some(Vec3::new(5.0, 1.6, 3.0)),
                    spatial: Some(true),
                    gain: 0.3,
                    ..PlayRequest::default()
                },
            ) {
                return Err("latency cue refused".into());
            }
            let mut first_signal = None;
            while started.elapsed() < Duration::from_millis(250) {
                let count = poll_pcm(audio, settings, &mut scratch);
                if scratch
                    .get(..count)
                    .unwrap_or_default()
                    .iter()
                    .any(|pair| pair.iter().any(|sample| sample.abs() > 1.0e-9))
                {
                    first_signal = Some(started.elapsed().as_secs_f64() * 1_000.0_f64);
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            audio.clear_world();
            audio.set_acoustic_scene(AcousticScene::build(&fixture("core:pool_tile_deck_01")?));
            let _cleared = drain_for(audio, settings, &mut scratch, 60);
            if !audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(9999)),
                    position: Some(Vec3::new(5.0, 1.6, 3.0)),
                    spatial: Some(true),
                    looping: Some(true),
                    gain: 0.1,
                    ..PlayRequest::default()
                },
            ) {
                return Err("latency loop refused".into());
            }
            if !drain_for(audio, settings, &mut scratch, 250) {
                return Err("latency loop produced no native signal".into());
            }
            settings.muted = true;
            let muted_at = Instant::now();
            let mut zeros = 0_usize;
            let mut confirmed_silence = None;
            while muted_at.elapsed() < Duration::from_millis(250) {
                let count = poll_pcm(audio, settings, &mut scratch);
                for pair in scratch.get(..count).unwrap_or_default() {
                    if pair.iter().all(|sample| sample.abs().to_bits() == 0) {
                        zeros = zeros.saturating_add(1);
                    } else {
                        zeros = 0;
                    }
                }
                if zeros >= 480 {
                    confirmed_silence = Some(muted_at.elapsed().as_secs_f64() * 1_000.0_f64);
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            results.push(serde_json::json!({"mode":format!("{mode:?}"),"trial":trial,"play_to_first_signal_ms":first_signal,"mute_to_10ms_silence_confirmed_ms":confirmed_silence,"requested_poll_ms":1_u8,"physical_latency_measured":false}));
            if first_signal.is_none() || confirmed_silence.is_none() {
                return Err("native latency witness timed out".into());
            }
        }
    }
    audio.clear_world();
    std::fs::write(
        out.join("latency.json"),
        serde_json::to_vec_pretty(&results).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
fn poll_pcm(audio: &mut AudioSystem, settings: AudioSettings, scratch: &mut [[f32; 2]]) -> usize {
    audio.update(
        Listener {
            position: Vec3::new(4.0, 1.6, 3.0),
            ..Listener::default()
        },
        false,
        1.0,
        settings,
    );
    audio.drain_capture(scratch)
}
fn drain_for(
    audio: &mut AudioSystem,
    settings: AudioSettings,
    scratch: &mut [[f32; 2]],
    milliseconds: u64,
) -> bool {
    let start = Instant::now();
    let mut signal = false;
    while start.elapsed() < Duration::from_millis(milliseconds) {
        let count = poll_pcm(audio, settings, scratch);
        signal |= scratch
            .get(..count)
            .unwrap_or_default()
            .iter()
            .any(|pair| pair.iter().any(|sample| sample.abs() > 1.0e-9));
        std::thread::sleep(Duration::from_millis(1));
    }
    signal
}

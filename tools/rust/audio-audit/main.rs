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
    let mode = args.next();
    let probe = mode
        .as_ref()
        .is_some_and(|value| value == "--control-probe");
    let lifetime = mode
        .as_ref()
        .is_some_and(|value| value == "--lifetime-probe");
    let sustained = mode
        .as_ref()
        .is_some_and(|value| value == "--sustained-probe");
    let fallback = mode
        .as_ref()
        .is_some_and(|value| value == "--fallback-probe");
    if mode.is_some() && !probe && !lifetime && !sustained && !fallback {
        return Err("Unknown audit mode".into());
    }
    if args.next().is_some() || out.exists() {
        return Err("Use a new output directory and exactly two arguments".into());
    }
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    let heap_before_audio = LIVE_BYTES.load(Ordering::Relaxed);
    let mut audio = AudioSystem::new(&assets, &sdl);
    let heap_after_audio = LIVE_BYTES.load(Ordering::Relaxed);
    if fallback {
        let initialized = audio.stats();
        audio.unload();
        let result = serde_json::json!({"initialized":initialized,"unloaded":audio.stats(),"heap_before_audio":heap_before_audio,"heap_after_unload":LIVE_BYTES.load(Ordering::Relaxed),"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed)});
        std::fs::write(
            out.join("fallback.json"),
            serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }
    if !audio.stats().enabled {
        return Err("Native output unavailable; inspect actual device log".into());
    }
    if lifetime {
        return lifetime_probe(&mut audio, &sdl, &assets, &out);
    }
    if sustained {
        return sustained_probe(&mut audio, &out);
    }
    if probe {
        control_probe(&mut audio, &out)?;
        audio.unload();
        let allocator = serde_json::json!({"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"heap_after_unload":LIVE_BYTES.load(Ordering::Relaxed)});
        std::fs::write(
            out.join("control-allocator.json"),
            serde_json::to_vec_pretty(&allocator).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
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

// Explicit native control-pressure witness. The trailing observation deliberately
// performs no updates: an already requested mute must survive a game-thread stall.
fn control_probe(audio: &mut AudioSystem, out: &std::path::Path) -> Result<(), String> {
    let mut rows = Vec::new();
    let mut scratch = vec![[0.0_f32; 2]; 96_000];
    for mode in [SpatialMode::Speakers, SpatialMode::Headphones] {
        for trial in 0_u8..8 {
            let settings = AudioSettings {
                spatial_mode: mode,
                ..AudioSettings::default()
            };
            audio.clear_world();
            audio.set_acoustic_scene(AcousticScene::build(&fixture("core:pool_tile_deck_01")?));
            audio.update(Listener::default(), false, 1.0, settings);
            if !audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(100)),
                    position: Some(Vec3::new(2.0, 1.6, 2.0)),
                    looping: Some(true),
                    gain: 0.5,
                    ..PlayRequest::default()
                },
            ) {
                return Err("control-pressure loop refused".into());
            }
            if !drain_for(audio, settings, &mut scratch, 200) {
                return Err("control-pressure witness has no native signal".into());
            }
            // Align near the start of a newly observed callback, leaving its
            // next control boundary enough room for a bounded main-thread burst.
            let frames = audio.stats().output_frames;
            let wait = Instant::now();
            while audio.stats().output_frames == frames
                && wait.elapsed() < Duration::from_millis(100)
            {
                std::hint::spin_loop();
            }
            let _drained = audio.drain_capture(&mut scratch);
            let before = audio.stats();
            let burst = Instant::now();
            for _ in 0_u16..128 {
                audio.update(Listener::default(), false, 1.0, settings);
            }
            audio.update(
                Listener::default(),
                false,
                1.0,
                AudioSettings {
                    muted: true,
                    ..settings
                },
            );
            let burst_us = burst.elapsed().as_secs_f64() * 1_000_000.0_f64;
            // Owned device playback continues while this thread is stalled.
            std::thread::sleep(Duration::from_millis(100));
            let count = audio.drain_capture(&mut scratch);
            let trailing = scratch
                .get(count.saturating_sub(960)..count)
                .unwrap_or_default();
            let tail_peak = trailing
                .iter()
                .flatten()
                .map(|v| v.abs())
                .fold(0.0_f32, f32::max);
            let after = audio.stats();
            rows.push(serde_json::json!({"mode":format!("{mode:?}"),"trial":trial,"burst_us":burst_us,"trailing_20ms_peak_after_100ms":tail_peak,"requested_mute_delivered":!trailing.is_empty() && tail_peak.to_bits()==0,"queue_full_delta":after.parameter_queue_full.saturating_sub(before.parameter_queue_full),"frames_captured":count,"errors":after.errors}));
            // Restore via the ordinary update before the next trial.
            let _quiet = drain_for(
                audio,
                AudioSettings {
                    muted: true,
                    ..settings
                },
                &mut scratch,
                60,
            );
        }
    }
    audio.clear_world();
    std::fs::write(
        out.join("control-pressure.json"),
        serde_json::to_vec_pretty(&rows).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

// Explicit device/lifetime campaign, separate from ordinary tests and timing.
// Every reload's preparation thread is joined before main-thread SDL ownership.
fn lifetime_probe(
    audio: &mut AudioSystem,
    sdl: &sdl3::Sdl,
    assets: &std::path::Path,
    out: &std::path::Path,
) -> Result<(), String> {
    let mut rows: Vec<(u16, u64, u64, places::engine::audio::AudioStats)> = Vec::with_capacity(48);
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    audio.unload();
    let started = Instant::now();
    for cycle in 0_u16..48 {
        let before = LIVE_BYTES.load(Ordering::Relaxed);
        let root = assets.to_path_buf();
        let prepared = std::thread::Builder::new()
            .name("audit-audio-prepare".into())
            .spawn(move || AudioSystem::prepare(&root, || false))
            .map_err(|error| error.to_string())?
            .join()
            .map_err(|_panic| "Audio preparation worker panicked".to_string())?;
        *audio = AudioSystem::from_prepared(prepared, sdl);
        if !audio.stats().enabled {
            return Err("reload did not restore the actual native device".into());
        }
        let settings = AudioSettings {
            spatial_mode: if cycle % 2 == 0 {
                SpatialMode::Speakers
            } else {
                SpatialMode::Headphones
            },
            ..AudioSettings::default()
        };
        let level = fixture(if cycle % 2 == 0 {
            "core:pool_tile_deck_01"
        } else {
            "core:carpet_beige_01"
        })?;
        audio.set_acoustic_scene(AcousticScene::build(&level));
        audio.update(Listener::default(), false, 1.0, settings);
        for source in 0_u16..48 {
            if !audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(u64::from(source))),
                    position: Some(Vec3::new(4.0, 2.0, 7.0)),
                    looping: Some(true),
                    gain: 0.5,
                    ..PlayRequest::default()
                },
            ) {
                return Err("lifetime full-capacity start refused".into());
            }
        }
        if !drain_for(audio, settings, &mut scratch, 200) {
            return Err("lifetime playback lacked native PCM".into());
        }
        if cycle == 0 {
            freshness_probe(audio, settings, out)?;
        }
        // Fill asynchronous retirement/pending ownership, then cancel that
        // entire scene before the next callback. This uses the real queue gate.
        for source in 0_u16..48 {
            let _replaced = audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(u64::from(source))),
                    position: Some(Vec3::new(4.0, 2.0, 7.0)),
                    looping: Some(true),
                    gain: 0.5,
                    ..PlayRequest::default()
                },
            );
        }
        let pressure = audio.stats();
        audio.clear_world();
        let cleared = audio.stats();
        if cleared.active_voices != 0
            || cleared.pending_voices != 0
            || cleared.acoustic_surfaces != 0
            || cleared.loaded_clips != 66
            || cleared.decoded_bytes != 43_409_440
        {
            return Err("scene clear lost shared cues or retained world owners".into());
        }
        let _drained = drain_for(audio, settings, &mut scratch, 60);
        audio.unload();
        if audio.stats().decoded_bytes != 0 || audio.stats().enabled {
            return Err("unload retained native ownership".into());
        }
        drop(level);
        rows.push((cycle, before, LIVE_BYTES.load(Ordering::Relaxed), pressure));
    }
    std::fs::write(
        out.join("reload-owners.json"),
        serde_json::to_vec_pretty(&rows).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let summary = serde_json::json!({"seconds":started.elapsed().as_secs_f64(),"reloads":rows.len(),"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"unloaded":audio.stats()});
    std::fs::write(
        out.join("reload-summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if CALLBACK_ALLOCS.load(Ordering::Relaxed) > 0 || CALLBACK_FREES.load(Ordering::Relaxed) > 0 {
        return Err("callback allocator activity during reload".into());
    }
    Ok(())
}

type SustainedSample = (SpatialMode, f64, places::engine::audio::AudioStats, u64);
type SustainedInterval = (
    SpatialMode,
    usize,
    places::engine::audio::AudioStats,
    places::engine::audio::AudioStats,
    places::engine::audio::AudioStats,
    f64,
);

// Continuous actual-device playback. Wait for the documented pending/retired
// boundary before counting steady-state voices, and retain each completed mode.
fn sustained_probe(audio: &mut AudioSystem, out: &std::path::Path) -> Result<(), String> {
    let seconds = std::env::var("PLACES_AUDIO_AUDIT_SECONDS").map_or(Ok(300_u64), |value| {
        value.parse::<u64>().map_err(|error| error.to_string())
    })?;
    if !(1..=3600).contains(&seconds) {
        return Err("Audio audit seconds must be in1..=3600 per mode".into());
    }
    let capacity = usize::try_from(
        seconds
            .saturating_div(30)
            .saturating_add(2)
            .saturating_mul(2),
    )
    .map_err(|error| error.to_string())?;
    let mut rows: Vec<SustainedSample> = Vec::with_capacity(capacity);
    let mut intervals: Vec<SustainedInterval> = Vec::with_capacity(2);
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    let listener = Listener {
        position: Vec3::new(4.0, 1.6, 3.0),
        ..Listener::default()
    };
    for mode in [SpatialMode::Speakers, SpatialMode::Headphones] {
        let settings = AudioSettings {
            spatial_mode: mode,
            ..AudioSettings::default()
        };
        let (starting, settle_ms) = begin_sustained_mode(audio, settings, listener, &mut scratch)?;
        let before = audio.stats();
        let signals =
            sustain_interval(audio, settings, listener, seconds, &mut scratch, &mut rows)?;
        let after = audio.stats();
        if signals == 0 || after.output_frames <= before.output_frames {
            return Err("Sustained output did not advance".into());
        }
        intervals.push((mode, signals, before, after, starting, settle_ms));
        let progress =
            serde_json::json!({"seconds_per_mode":seconds,"samples":rows,"intervals":intervals});
        std::fs::write(
            out.join("sustained.json"),
            serde_json::to_vec_pretty(&progress).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
    }
    audio.clear_world();
    audio.unload();
    let summary = serde_json::json!({"seconds_per_mode":seconds,"completed_modes":intervals.len(),"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"subjective_listening":false,"settings_change_and_device_hotplug":false,"samples_include_fixed_harness_capacity":true});
    std::fs::write(
        out.join("sustained-summary.json"),
        serde_json::to_vec_pretty(&summary).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if CALLBACK_ALLOCS.load(Ordering::Relaxed) > 0 || CALLBACK_FREES.load(Ordering::Relaxed) > 0 {
        return Err("Sustained callback allocator activity".into());
    }
    Ok(())
}

fn begin_sustained_mode(
    audio: &mut AudioSystem,
    settings: AudioSettings,
    listener: Listener,
    scratch: &mut [[f32; 2]],
) -> Result<(places::engine::audio::AudioStats, f64), String> {
    audio.clear_world();
    audio.set_acoustic_scene(AcousticScene::build(&fixture("core:pool_tile_deck_01")?));
    audio.update(listener, false, 1.0, settings);
    for source in 0_u16..48 {
        if !audio.play(
            "fluorescent_buzz",
            PlayRequest {
                source: Some(SourceId(u64::from(source))),
                position: Some(Vec3::new(4.0, 2.0, 7.0)),
                looping: Some(true),
                gain: 0.5,
                ..PlayRequest::default()
            },
        ) {
            return Err("Sustained voice refused".into());
        }
    }
    let starting = audio.stats();
    let settling = Instant::now();
    while (audio.stats().active_voices != 48 || audio.stats().pending_voices != 0)
        && settling.elapsed() < Duration::from_secs(2)
    {
        audio.update(listener, false, 1.0, settings);
        let _drained = audio.drain_capture(scratch);
        std::thread::sleep(Duration::from_millis(5));
    }
    let settled = audio.stats();
    if settled.active_voices != 48 || settled.pending_voices != 0 || settled.errors != 0 {
        return Err(format!(
            "Sustained start did not settle: {}",
            serde_json::to_string(&settled).map_err(|error| error.to_string())?
        ));
    }
    Ok((starting, settling.elapsed().as_secs_f64() * 1_000.0_f64))
}

fn sustain_interval(
    audio: &mut AudioSystem,
    settings: AudioSettings,
    listener: Listener,
    seconds: u64,
    scratch: &mut [[f32; 2]],
    rows: &mut Vec<SustainedSample>,
) -> Result<usize, String> {
    let started = Instant::now();
    let mut signals = 0_usize;
    let mut sample_at = 0_u64;
    while started.elapsed() < Duration::from_secs(seconds) {
        audio.update(listener, false, 1.0, settings);
        let count = audio.drain_capture(scratch);
        signals = signals.saturating_add(
            scratch
                .get(..count)
                .unwrap_or_default()
                .iter()
                .filter(|pair| pair.iter().any(|value| value.abs() > 1.0e-9))
                .count(),
        );
        if started.elapsed().as_secs() >= sample_at {
            let stats = audio.stats();
            if stats.active_voices != 48 || stats.errors != 0 {
                return Err(format!(
                    "Sustained playback lost a voice or reported an error: {}",
                    serde_json::to_string(&stats).map_err(|error| error.to_string())?
                ));
            }
            rows.push((
                settings.spatial_mode,
                started.elapsed().as_secs_f64(),
                stats,
                LIVE_BYTES.load(Ordering::Relaxed),
            ));
            sample_at = sample_at.saturating_add(30);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(signals)
}

fn freshness_probe(
    audio: &mut AudioSystem,
    settings: AudioSettings,
    out: &std::path::Path,
) -> Result<(), String> {
    for source in 0_u16..48 {
        audio.set_source_position(SourceId(u64::from(source)), Vec3::new(4.0, 2.0, 3.0));
    }
    std::thread::sleep(Duration::from_millis(150));
    let stalled = audio.stats();
    if stalled.stale_paths != 48 || stalled.oldest_stale_path_nanos < 150_000_000 {
        return Err("wall-clock dirty-path witness omitted a stalled source".into());
    }
    let started = Instant::now();
    while audio.stats().stale_paths != 0 && started.elapsed() < Duration::from_secs(2) {
        let _captured = poll_pcm(audio, settings, &mut [[0.0_f32; 2]; 512]);
        std::thread::sleep(Duration::from_millis(25));
    }
    let refreshed = audio.stats();
    let refresh_ms = started.elapsed().as_secs_f64() * 1_000.0_f64;
    if refreshed.stale_paths != 0
        || audio
            .voice_snapshot()
            .iter()
            .any(|voice| voice.acoustic_path.transmission < 0.999)
    {
        return Err("stale path failed to converge to its current source".into());
    }
    let scene = AcousticScene::build(&fixture("core:carpet_beige_01")?);
    let expected = scene.path(Vec3::new(4.0, 1.6, 3.0), Vec3::new(4.0, 2.0, 7.0));
    audio.set_acoustic_scene(scene);
    for source in 0_u16..48 {
        audio.set_source_position(SourceId(u64::from(source)), Vec3::new(4.0, 2.0, 7.0));
    }
    let switched = Instant::now();
    while audio.stats().stale_paths != 0 && switched.elapsed() < Duration::from_secs(2) {
        let _captured = poll_pcm(audio, settings, &mut [[0.0_f32; 2]; 512]);
        std::thread::sleep(Duration::from_millis(25));
    }
    let scene_done = audio.stats();
    if scene_done.stale_paths != 0
        || audio.voice_snapshot().iter().any(|voice| {
            voice.acoustic_path.transmission.to_bits() != expected.transmission.to_bits()
                || voice.acoustic_path.cutoff_hz.to_bits() != expected.cutoff_hz.to_bits()
        })
    {
        return Err("scene replacement retained a previous path".into());
    }
    let result = serde_json::json!({"stalled":stalled,"refreshed":refreshed,"refresh_after_stall_ms":refresh_ms,"scene_replaced":scene_done,"expected_path":expected,"wall_time_bound_claimed":false});
    std::fs::write(
        out.join("path-freshness.json"),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

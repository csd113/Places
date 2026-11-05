//! Explicit allocator census. Fixed atomic storage; no callback locks or I/O.
use super::{
    AcousticScene, AudioSettings, AudioSystem, CALLBACK_ALLOCS, CALLBACK_FREES, LIVE_BYTES,
    Listener, fixture,
};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

const SLOTS: usize = 16_384;
static ENABLED: AtomicBool = AtomicBool::new(false);
static PHASE: AtomicUsize = AtomicUsize::new(0);
static OVERFLOW: AtomicU64 = AtomicU64::new(0);
struct Allocation {
    pointer: AtomicUsize,
    bytes: AtomicUsize,
    phase: AtomicUsize,
}
static ALLOCATIONS: [Allocation; SLOTS] = [const {
    Allocation {
        pointer: AtomicUsize::new(0),
        bytes: AtomicUsize::new(0),
        phase: AtomicUsize::new(0),
    }
}; SLOTS];

pub fn insert(pointer: *mut u8, bytes: usize) {
    let phase = PHASE.load(Ordering::Relaxed);
    if phase == 0 {
        return;
    }
    let address = pointer.addr();
    let start = address / 16;
    for offset in 0..SLOTS {
        if let Some(slot) = ALLOCATIONS.get(start.saturating_add(offset) % SLOTS) {
            let previous = slot.pointer.load(Ordering::Relaxed);
            if previous <= 1
                && slot
                    .pointer
                    .compare_exchange(previous, address, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                slot.bytes.store(bytes, Ordering::Relaxed);
                slot.phase.store(phase, Ordering::Release);
                return;
            }
        }
    }
    let _overflow = OVERFLOW.fetch_add(1, Ordering::Relaxed);
}
pub fn remove(pointer: *mut u8) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let address = pointer.addr();
    let start = address / 16;
    for offset in 0..SLOTS {
        if let Some(slot) = ALLOCATIONS.get(start.saturating_add(offset) % SLOTS) {
            let stored = slot.pointer.load(Ordering::Relaxed);
            if stored == 0 {
                return;
            }
            if stored == address {
                slot.phase.store(0, Ordering::Relaxed);
                slot.pointer.store(1, Ordering::Release);
                return;
            }
        }
    }
}
#[derive(Clone, Copy, Default, serde::Serialize)]
struct Sample {
    live_rust_bytes: u64,
    sdl_live_allocations: i32,
    phase_bytes: [usize; 8],
    phase_allocations: [usize; 8],
}
fn sample() -> Sample {
    let mut sample = Sample {
        live_rust_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        sdl_live_allocations: sdl3::sys::stdinc::SDL_GetNumAllocations(),
        ..Sample::default()
    };
    for slot in &ALLOCATIONS {
        if slot.pointer.load(Ordering::Acquire) <= 1 {
            continue;
        }
        let phase = slot.phase.load(Ordering::Acquire);
        if let Some(total) = sample.phase_bytes.get_mut(phase) {
            *total = total.saturating_add(slot.bytes.load(Ordering::Relaxed));
        }
        if let Some(count) = sample.phase_allocations.get_mut(phase) {
            *count = count.saturating_add(1);
        }
    }
    sample
}
pub fn run(assets: &std::path::Path, out: &std::path::Path) -> Result<(), String> {
    let mut samples = [Sample::default(); 15];
    let mut scratch = [[0.0_f32; 2]; 512];
    let sdl = sdl3::init().map_err(|error| error.to_string())?;
    if let Some(row) = samples.get_mut(0) {
        *row = sample();
    }
    ENABLED.store(true, Ordering::Relaxed);
    PHASE.store(1, Ordering::Relaxed);
    places::engine::logging::info("[audio-owner] explicit stdout warmup");
    if let Some(row) = samples.get_mut(1) {
        *row = sample();
    }
    PHASE.store(7, Ordering::Relaxed);
    let path = assets.join("audio/switch_click/01.wav");
    let data = kira::sound::static_sound::StaticSoundData::from_cursor(std::io::Cursor::new(
        std::fs::read(&path).map_err(|error| error.to_string())?,
    ))
    .map_err(|error| error.to_string())?;
    drop(data);
    drop(path);
    for cycle in 0_usize..12 {
        PHASE.store(2, Ordering::Relaxed);
        let prepared = AudioSystem::prepare(assets, || false);
        PHASE.store(3, Ordering::Relaxed);
        let mut audio = AudioSystem::from_prepared(prepared, &sdl);
        if !audio.stats().enabled {
            return Err("owner probe native device unavailable".into());
        }
        PHASE.store(4, Ordering::Relaxed);
        let level = fixture("core:pool_tile_deck_01")?;
        audio.set_acoustic_scene(AcousticScene::build(&level));
        let settings = AudioSettings::default();
        audio.update(Listener::default(), false, 1.0, settings);
        PHASE.store(5, Ordering::Relaxed);
        let _settled = super::start_capacity(&mut audio, settings, &mut scratch, 100)
            .map_err(|error| format!("owner cycle {cycle}: {error}"))?;
        audio.clear_world();
        audio.unload();
        drop(level);
        drop(audio);
        PHASE.store(6, Ordering::Relaxed);
        if AudioSystem::prepare(assets, || true).is_ok() {
            return Err("cancelled preparation succeeded".into());
        }
        if cycle > 0 {
            let failed = AudioSystem::from_prepared(
                Err("controlled owner-probe preparation failure".into()),
                &sdl,
            );
            if failed.stats().enabled || failed.stats().errors != 1 {
                return Err("preparation error failed silent fallback".into());
            }
            drop(failed);
        }
        if let Some(row) = samples.get_mut(cycle.saturating_add(2)) {
            *row = sample();
        }
    }
    drop(sdl);
    if let Some(row) = samples.get_mut(14) {
        *row = sample();
    }
    PHASE.store(0, Ordering::Relaxed);
    let allocations: Vec<_> = ALLOCATIONS.iter().filter(|slot| slot.pointer.load(Ordering::Acquire) > 1).map(|slot| serde_json::json!({"bytes":slot.bytes.load(Ordering::Relaxed),"phase":slot.phase.load(Ordering::Relaxed)})).collect();
    let result = serde_json::json!({"samples":samples,"surviving_tracked_allocations":allocations,"overflow":OVERFLOW.load(Ordering::Relaxed),"phases":["untracked harness/pre-audio","stdout warmup","catalog/decode preparation","native mixer/device installation","scene construction","voice/control playback","cancelled prepare","single-file decoder warmup"],"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"full_capacity_admitted_cycles":12_u8,"native_pcm_cycles":12_u8,"native_caveat":"SDL counts of -1 are unavailable; nonnegative counts are allocation counts, not bytes; driver/CRT/process memory is outside Rust allocator accounting"});
    std::fs::write(
        out.join("owners.json"),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    if OVERFLOW.load(Ordering::Relaxed) != 0 {
        return Err("owner census overflow".into());
    }
    Ok(())
}

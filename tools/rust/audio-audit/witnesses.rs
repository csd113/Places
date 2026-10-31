//! Explicit native clear/stall/restart and moving-path witnesses, outside CI.
use super::{
    AcousticScene, AudioSettings, AudioSystem, CALLBACK_ALLOCS, CALLBACK_FREES, Listener,
    PlayRequest, SourceId, SpatialMode, Vec3, begin_sustained_mode, drain_for, fixture,
};
use places::engine::audio::AudioStats;
use places::engine::level::LevelDef;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
struct Window {
    ms: f64,
    frames: usize,
    peak: f32,
    energy: f64,
    stats: AudioStats,
}
fn capture_only(audio: &mut AudioSystem, milliseconds: u64) -> Vec<Window> {
    let started = Instant::now();
    let mut rows = Vec::new();
    let mut scratch = [[0.0_f32; 2]; 2048];
    while started.elapsed() < Duration::from_millis(milliseconds) {
        let count = audio.drain_capture(&mut scratch);
        if count > 0 {
            let pcm = scratch.get(..count).unwrap_or_default();
            rows.push(Window {
                ms: started.elapsed().as_secs_f64() * 1_000.0,
                frames: count,
                peak: pcm
                    .iter()
                    .flatten()
                    .map(|value| value.abs())
                    .fold(0.0, f32::max),
                energy: pcm
                    .iter()
                    .flatten()
                    .map(|value| f64::from(*value).powi(2))
                    .sum(),
                stats: audio.stats(),
            });
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    rows
}
pub fn run(audio: &mut AudioSystem, out: &std::path::Path) -> Result<(), String> {
    let mut trials = Vec::new();
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
        let _started = begin_sustained_mode(audio, settings, listener, &mut scratch)?;
        for source in 0_u16..48 {
            audio.set_source_position(SourceId(u64::from(source)), Vec3::new(4.0, 2.0, 3.0));
        }
        let _settled = drain_for(audio, settings, &mut scratch, 350);
        let before = audio.stats();
        let _discarded = audio.drain_capture(&mut scratch);
        for source in 0_u16..48 {
            audio.set_source_position(SourceId(u64::from(source)), Vec3::new(4.0, 2.0, 7.0));
        }
        let moving_stall = capture_only(audio, 450);
        let stalled = audio.stats();
        let mut opportunities = Vec::new();
        let refresh = Instant::now();
        while audio.stats().stale_paths > 0 && refresh.elapsed() < Duration::from_secs(2) {
            let prior = audio.stats();
            audio.update(listener, false, 1.0, settings);
            let after = audio.stats();
            if after
                .acoustic_queries
                .saturating_sub(prior.acoustic_queries)
                > 8
            {
                return Err("path query cap exceeded".into());
            }
            opportunities.push((refresh.elapsed().as_secs_f64() * 1_000.0_f64, after));
            std::thread::sleep(Duration::from_millis(25));
        }
        let expected = AcousticScene::build(&fixture("core:pool_tile_deck_01")?)
            .path(listener.position, Vec3::new(4.0, 2.0, 7.0));
        if audio.stats().stale_paths != 0
            || audio.voice_snapshot().iter().any(|voice| {
                voice.acoustic_path.transmission.to_bits() != expected.transmission.to_bits()
                    || voice.acoustic_path.cutoff_hz.to_bits() != expected.cutoff_hz.to_bits()
            })
        {
            return Err("path recovery changed material semantics".into());
        }
        let _recovered = drain_for(audio, settings, &mut scratch, 350);
        let _pre_clear_discarded = audio.drain_capture(&mut scratch);
        let pre_clear = audio.stats();
        audio.clear_world();
        // Intentionally no update, play, door or source control until capture ends.
        let clear_stall = capture_only(audio, 600);
        let cleared = audio.stats();
        if cleared.active_voices != 0
            || cleared.pending_voices != 0
            || cleared.acoustic_surfaces != 0
            || cleared.loaded_clips != 66
        {
            return Err("clear retained owners or discarded shared cues".into());
        }
        let _restart = begin_sustained_mode(audio, settings, listener, &mut scratch)?;
        if !drain_for(audio, settings, &mut scratch, 250) {
            return Err("restart produced no native PCM".into());
        }
        trials.push(serde_json::json!({"mode":mode,"before":before,"moving_stall":moving_stall,"stalled":stalled,"refresh_opportunities":opportunities,"expected":expected,"pre_clear":pre_clear,"clear_stall":clear_stall,"cleared":cleared,"restarted":audio.stats()}));
    }
    let result = serde_json::json!({"trials":trials,"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"tap_is_physical_latency":false});
    std::fs::write(
        out.join("disposal-stall.json"),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    audio.unload();
    Ok(())
}

pub fn scene(audio: &mut AudioSystem, out: &std::path::Path) -> Result<(), String> {
    use places::engine::door::Doors;
    let path = std::env::var_os("PLACES_AUDIO_AUDIT_LEVEL")
        .ok_or("scene probe requires PLACES_AUDIO_AUDIT_LEVEL")?;
    let level =
        LevelDef::from_json(&std::fs::read_to_string(&path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let mut rows = Vec::with_capacity(2);
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    for mode in [SpatialMode::Speakers, SpatialMode::Headphones] {
        audio.clear_world();
        let mut doors = Doors::from_level(&level);
        let mut oracle = AcousticScene::build(&level);
        audio.set_acoustic_scene(AcousticScene::build(&level));
        let settings = AudioSettings {
            spatial_mode: mode,
            ..AudioSettings::default()
        };
        let mut listener = Listener {
            position: Vec3::new(level.spawn.x, 1.6, level.spawn.z),
            ..Listener::default()
        };
        audio.update(listener, false, 1.0, settings);
        for source in 0_u16..48 {
            let _accepted = audio.play(
                "fluorescent_buzz",
                PlayRequest {
                    source: Some(SourceId(u64::from(source))),
                    position: Some(Vec3::new(
                        listener.position.x,
                        listener.position.y + 0.4,
                        listener.position.z + 2.0,
                    )),
                    gain: 0.5,
                    looping: Some(true),
                    ..PlayRequest::default()
                },
            );
        }
        let start = Instant::now();
        let mut update_costs = Vec::with_capacity(1000);
        let mut max_queries = 0_u64;
        let mut signal = 0_usize;
        let mut stalled = None;
        let mut step = 0_u16;
        let before = audio.stats();
        while start.elapsed() < Duration::from_secs(20) {
            move_scene(audio, &level, &mut doors, &mut oracle, &mut listener, step);
            if step == 80 {
                stalled = Some(capture_only(audio, 250));
            }
            let prior = audio.stats();
            let update = Instant::now();
            audio.update(listener, false, 1.0, settings);
            update_costs.push(update.elapsed().as_secs_f64() * 1_000_000.0_f64);
            let after = audio.stats();
            max_queries = max_queries.max(
                after
                    .acoustic_queries
                    .saturating_sub(prior.acoustic_queries),
            );
            let count = audio.drain_capture(&mut scratch);
            signal = signal.saturating_add(
                scratch
                    .get(..count)
                    .unwrap_or_default()
                    .iter()
                    .filter(|pair| pair.iter().any(|value| value.abs() > 1.0e-9))
                    .count(),
            );
            step = step.saturating_add(1);
            std::thread::sleep(Duration::from_millis(25));
        }
        let settling = Instant::now();
        while audio.stats().stale_paths > 0 && settling.elapsed() < Duration::from_secs(2) {
            audio.update(listener, false, 1.0, settings);
            let _captured = audio.drain_capture(&mut scratch);
            std::thread::sleep(Duration::from_millis(25));
        }
        let snapshots = audio.voice_snapshot();
        if max_queries > 8 || audio.stats().stale_paths != 0 || snapshots.len() != 48 || signal == 0
        {
            return Err("real-scene query cap/settlement/voice/PCM failure".into());
        }
        validate_paths(&snapshots, &oracle, &listener)?;
        rows.push(serde_json::json!({"mode":mode,"before":before,"after":audio.stats(),"max_queries_per_update":max_queries,"update_us":update_costs,"stalled":stalled,"settled_voices":snapshots,"signals":signal,"seconds":start.elapsed().as_secs_f64()}));
    }
    audio.clear_world();
    audio.unload();
    let result = serde_json::json!({"level":level.id,"scene_counts":AcousticScene::build(&level).counts(),"door_count":level.doors.len(),"rows":rows,"callback_allocations":CALLBACK_ALLOCS.load(Ordering::Relaxed),"callback_deallocations":CALLBACK_FREES.load(Ordering::Relaxed),"caveat":"actual authored acoustic geometry and animated door leaves; scripted source/listener path, no physics/localization/listening claim"});
    std::fs::write(
        out.join("scene.json"),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn move_scene(
    audio: &mut AudioSystem,
    level: &LevelDef,
    doors: &mut places::engine::door::Doors,
    oracle: &mut AcousticScene,
    listener: &mut Listener,
    step: u16,
) {
    if step.is_multiple_of(80) {
        for door in &level.doors {
            let _toggled = doors.toggle(&door.id);
        }
    }
    let _moved = doors.advance(0.025, |_, _| false);
    oracle.update_doors(doors);
    audio.update_acoustic_doors(doors);
    let phase = f32::from(step) * 0.015;
    listener.position = Vec3::new(
        phase.sin().mul_add(2.0, level.spawn.x),
        1.6,
        phase.cos().mul_add(2.0, level.spawn.z),
    );
    for source in 0_u16..48 {
        let offset = f32::from(source) * 0.131;
        audio.set_source_position(
            SourceId(u64::from(source)),
            Vec3::new(
                offset.sin().mul_add(5.0, listener.position.x),
                listener.position.y + 0.4,
                offset.cos().mul_add(5.0, listener.position.z),
            ),
        );
    }
}

fn validate_paths(
    snapshots: &[places::engine::audio::AudioVoiceSnapshot],
    oracle: &AcousticScene,
    listener: &Listener,
) -> Result<(), String> {
    for voice in snapshots {
        let expected = oracle.path(
            listener.position,
            Vec3::from_array(voice.position.ok_or("scene source lost position")?),
        );
        if expected.transmission.to_bits() != voice.acoustic_path.transmission.to_bits()
            || expected.cutoff_hz.to_bits() != voice.acoustic_path.cutoff_hz.to_bits()
            || expected.budget_exhausted != voice.acoustic_path.budget_exhausted
        {
            return Err("real scene path/door/material oracle differs".into());
        }
    }
    Ok(())
}

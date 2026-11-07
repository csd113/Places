//! Opt-in complete-loop witnesses through the existing native device and mixer.
use super::{AudioSettings, AudioSystem, Listener, PlayRequest, SourceId, SpatialMode};
use glam::Vec3;
use places::engine::audio::{AudioBus, AudioCatalog};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

const SOURCE: SourceId = SourceId(72_101);
const CUES: [&str; 4] = [
    "empty_room",
    "threshold_air",
    "pale_signal",
    "below_the_landing",
];

fn start(audio: &mut AudioSystem, cue: &str, label: &str, spatial: bool) -> Result<(), String> {
    if !audio.play(
        cue,
        PlayRequest {
            source: Some(SOURCE),
            position: spatial.then_some(Vec3::new(1.0, 0.0, -1.0)),
            spatial: Some(spatial),
            looping: Some(true),
            bus: AudioBus::Ambient,
            ..PlayRequest::default()
        },
    ) {
        return Err(format!("Native ambient choice refused: {cue}"));
    }
    if label == "layered_ambience" {
        for (index, overlay) in CUES.iter().enumerate().skip(1) {
            let offset = u64::try_from(index).map_err(|error| error.to_string())?;
            if !audio.play(
                overlay,
                PlayRequest {
                    source: Some(SourceId(SOURCE.0.saturating_add(offset))),
                    spatial: Some(false),
                    looping: Some(true),
                    bus: AudioBus::Ambient,
                    ..PlayRequest::default()
                },
            ) {
                return Err(format!("Native layered ambience refused: {overlay}"));
            }
        }
    }
    Ok(())
}

fn capture(
    audio: &mut AudioSystem,
    out: &Path,
    cue: &str,
    label: &str,
    seconds: f64,
    settings: AudioSettings,
    spatial: bool,
) -> Result<Value, String> {
    let listener = Listener::default();
    let mut scratch = vec![[0.0_f32; 2]; 8192];
    audio.clear_world();
    audio.set_acoustic_scene(super::AcousticScene::default());
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(350) {
        audio.update(listener, false, 1.0, settings);
        let _discarded = audio.drain_capture(&mut scratch);
        std::thread::sleep(Duration::from_millis(5));
    }
    let before = audio.stats();
    start(audio, cue, label, spatial)?;
    let mut pcm = Vec::new();
    let mut snapshots = Vec::new();
    let started = Instant::now();
    let mut next_snapshot = 0.0_f64;
    let duration = Duration::from_secs_f64(seconds);
    while started.elapsed() < duration {
        audio.update(listener, false, 1.0, settings);
        let count = audio.drain_capture(&mut scratch);
        if pcm.len().saturating_add(count) > 3_072_000 {
            return Err("Ambient capture exceeds bounded 64 seconds".into());
        }
        pcm.extend_from_slice(scratch.get(..count).unwrap_or_default());
        if started.elapsed().as_secs_f64() >= next_snapshot {
            let voices = audio.voice_snapshot();
            if !voices
                .iter()
                .any(|voice| voice.source == Some(SOURCE.0) && voice.cue == cue && voice.looping)
            {
                return Err(format!("Native ambient loop lost ownership: {cue}"));
            }
            snapshots.push(json!({"seconds":started.elapsed().as_secs_f64(),"voices":voices,"stats":audio.stats()}));
            next_snapshot += 0.5_f64;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let peak = pcm.iter().flatten().fold(0.0_f32, |p, s| p.max(s.abs()));
    if pcm.is_empty()
        || pcm
            .iter()
            .flatten()
            .any(|s| !s.is_finite() || s.abs() > 1.0)
        || ((settings.muted || settings.master_volume == 0.0) && peak > 0.000_001)
        || (!(settings.muted || settings.master_volume == 0.0) && peak < 0.000_000_001)
    {
        return Err(format!(
            "Native ambient signal failed: {label}, peak={peak}"
        ));
    }
    let after = audio.stats();
    if after.errors != before.errors || after.capture_dropped != before.capture_dropped {
        return Err(format!("Native ambient callback/capture errors: {label}"));
    }
    super::write_wav(&out.join(format!("{label}.wav")), &pcm)?;
    audio.stop_source_with_fade(SOURCE, 0.12);
    let result = json!({"cue":cue,"label":label,"frames":pcm.len(),"peak":peak,"before":before,"after":after,"snapshots":snapshots,"subjective_listening":false});
    std::fs::write(
        out.join(format!("{label}.json")),
        serde_json::to_vec_pretty(&result).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(result)
}

pub fn run(
    audio: &mut AudioSystem,
    assets: &Path,
    out: &Path,
    reference_only: bool,
) -> Result<(), String> {
    let catalog = AudioCatalog::load(assets)?;
    let choices: Vec<_> = catalog.cue_ids().collect();
    let selected = if reference_only {
        CUES.get(..1).unwrap_or_default()
    } else {
        CUES.as_slice()
    };
    let mut results = Vec::new();
    for cue in selected {
        if !choices.contains(cue) {
            return Err(format!(
                "Ambient choice missing from normal catalog picker: {cue}"
            ));
        }
        results.push(capture(
            audio,
            out,
            cue,
            cue,
            if *cue == "empty_room" { 59.8 } else { 32.8 },
            AudioSettings {
                master_volume: 0.12,
                ..AudioSettings::default()
            },
            false,
        )?);
    }
    if !reference_only {
        results.push(capture(
            audio,
            out,
            "empty_room",
            "layered_ambience",
            16.8,
            AudioSettings::default(),
            false,
        )?);
        for (label, settings, spatial) in [
            (
                "muted",
                AudioSettings {
                    master_volume: 0.12,
                    muted: true,
                    ..AudioSettings::default()
                },
                false,
            ),
            (
                "master_zero",
                AudioSettings {
                    master_volume: 0.0,
                    ..AudioSettings::default()
                },
                false,
            ),
            (
                "spatial_speakers",
                AudioSettings {
                    master_volume: 0.12,
                    ..AudioSettings::default()
                },
                true,
            ),
            (
                "spatial_headphones",
                AudioSettings {
                    master_volume: 0.12,
                    spatial_mode: SpatialMode::Headphones,
                    ..AudioSettings::default()
                },
                true,
            ),
        ] {
            results.push(capture(
                audio,
                out,
                "pale_signal",
                label,
                1.0,
                settings,
                spatial,
            )?);
        }
    }
    audio.clear_world();
    audio.unload();
    let unloaded = audio.stats();
    if unloaded.active_voices != 0 || unloaded.pending_voices != 0 {
        return Err("Ambient unload retained voice ownership".into());
    }
    std::fs::write(
        out.join("ambient-summary.json"),
        serde_json::to_vec_pretty(&json!({"choices":choices,"results":results,"unloaded":unloaded,"subjective_listening":false}))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

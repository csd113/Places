//! Sequential original-cue audition through the real native mixer/device.
//! Reference composites never overlap runtime stems or enter the shipping bank.
use super::{AudioSettings, AudioSystem, Listener, PlayRequest, SourceId, SpatialMode};
use glam::Vec3;
use places::engine::audio::acoustics::AcousticScene;
use places::engine::audio::{AudioCatalog, PerceptionOwner};
use places::engine::level::LevelDef;
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scene {
    Front,
    Rear,
    Closed,
    Muted,
    Loop,
}
impl Scene {
    const fn label(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Rear => "rear",
            Self::Closed => "closed",
            Self::Muted => "muted",
            Self::Loop => "loop",
        }
    }
    const fn forced_seconds(self) -> Option<f64> {
        match self {
            Self::Muted => Some(0.3_f64),
            Self::Loop => Some(12.3_f64),
            Self::Front | Self::Rear | Self::Closed => None,
        }
    }
}
struct Capture<'a> {
    audio: &'a mut AudioSystem,
    out: &'a Path,
    scratch: Vec<[f32; 2]>,
}
impl Capture<'_> {
    fn poll(
        &mut self,
        listener: Listener,
        settings: AudioSettings,
        pcm: &mut Vec<[f32; 2]>,
    ) -> Result<(), String> {
        self.audio.update(listener, false, 1.0, settings);
        let count = self.audio.drain_capture(&mut self.scratch);
        if pcm.len().saturating_add(count) > 1_536_000 {
            return Err("Cue capture exceeds32seconds".into());
        }
        pcm.extend_from_slice(self.scratch.get(..count).unwrap_or_default());
        std::thread::sleep(Duration::from_millis(5));
        Ok(())
    }
    fn settle(&mut self, listener: Listener, settings: AudioSettings) {
        // Device commands and room/master fades are asynchronous. Exclude the
        // previous trial's queued output before evaluating a muted new cue.
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(200) {
            self.audio.update(listener, false, 1.0, settings);
            let _discarded = self.audio.drain_capture(&mut self.scratch);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn trial(&mut self, cue: &str, mode: SpatialMode, scene: Scene) -> Result<Value, String> {
        self.audio.clear_world();
        self.audio.set_acoustic_scene(if scene == Scene::Closed {
            closed_scene()?
        } else {
            AcousticScene::default()
        });
        let settings = AudioSettings {
            master_volume: 0.12,
            muted: scene == Scene::Muted,
            spatial_mode: mode,
            ..AudioSettings::default()
        };
        let (foot, position, listener) = emitters(cue, scene);
        self.settle(listener, settings);
        let _old_capture = self.audio.drain_capture(&mut self.scratch);
        let source = SourceId(42);
        if !self.audio.play(
            cue,
            PlayRequest {
                source: Some(source),
                position: Some(position),
                direction: (!foot).then_some(Vec3::Z),
                spatial: Some(true),
                looping: Some(scene == Scene::Loop),
                perception: PerceptionOwner::Gameplay { event: 1 },
                ..PlayRequest::default()
            },
        ) {
            return Err(format!("Native original cue refused:{cue}"));
        }
        let started = Instant::now();
        let mut pcm = Vec::new();
        let mut snapshots = Vec::new();
        let mut next_snapshot = 0.0_f64;
        loop {
            self.poll(listener, settings, &mut pcm)?;
            let elapsed = started.elapsed().as_secs_f64();
            if elapsed >= next_snapshot {
                snapshots.push(json!({"seconds":elapsed,"voices":self.audio.voice_snapshot(),"stats":self.audio.stats()}));
                next_snapshot += 0.25_f64;
            }
            if elapsed > 30.0_f64 {
                return Err(format!("Native cue did not finish:{cue}"));
            }
            if scene.forced_seconds().is_some_and(|end| elapsed >= end) {
                self.audio.stop_source_with_fade(source, 0.12);
                break;
            }
            if elapsed > 0.25_f64 && !self.audio.source_playing(source, cue) {
                break;
            }
        }
        let tail = Instant::now();
        while tail.elapsed() < Duration::from_millis(300) {
            self.poll(listener, settings, &mut pcm)?;
        }
        let peak = pcm
            .iter()
            .flatten()
            .map(|sample| sample.abs())
            .fold(0.0_f32, f32::max);
        if pcm.is_empty()
            || pcm
                .iter()
                .flatten()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
            || (scene == Scene::Muted && peak > 0.000_001)
            || (scene != Scene::Muted && peak < 0.000_000_001)
        {
            return Err(format!(
                "Native cue signal failed:{cue}/{} peak={peak} frames={}",
                scene.label(),
                pcm.len()
            ));
        }
        let energy = pcm.iter().flatten().fold(0.0_f64, |sum, value| {
            f64::from(*value).mul_add(f64::from(*value), sum)
        });
        let count =
            f64::from(u32::try_from(pcm.len().saturating_mul(2)).map_err(|e| e.to_string())?);
        let name = format!("{mode:?}-{}-{cue}.wav", scene.label());
        super::write_wav(&self.out.join(&name), &pcm)?;
        let result = json!({"cue":cue,"mode":format!("{mode:?}"),"scene":scene.label(),"seconds":started.elapsed().as_secs_f64(),"frames":pcm.len(),"peak":peak,"rms":(energy/count).sqrt(),"file":name,"snapshots":snapshots,"after":self.audio.stats()});
        self.audio.clear_world();
        writeln!(
            std::io::stdout(),
            "NATIVE_CUE {cue} {mode:?} {} peak={peak} frames={}",
            scene.label(),
            pcm.len()
        )
        .map_err(|e| e.to_string())?;
        Ok(result)
    }
}
fn emitters(cue: &str, scene: Scene) -> (bool, Vec3, Listener) {
    let foot = cue.contains("_foot_") || cue.contains("stomp") || cue.contains("gait");
    let position = if foot {
        Vec3::new(0.0, 0.055, 0.0)
    } else {
        Vec3::new(0.0, 3.1, 0.0)
    };
    let listener = Listener {
        position: Vec3::new(0.0, 1.6, if scene == Scene::Rear { -4.0 } else { 4.0 }),
        ..Listener::default()
    };
    (foot, position, listener)
}
fn closed_scene() -> Result<AcousticScene, String> {
    let level = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"tg_cue_occlusion","name":"Native original cue occlusion","spawn":{"x":0,"z":4},"defaults":{"wall":"core:wallpaper_yellow_01","floor":"home:tile_home_01","ceiling":"core:ceiling_panel_01"},"rooms":[{"x":-8,"z":-8,"width":16,"depth":16,"height":5}],"walls":[{"x":-8,"z":2,"width":16,"depth":0.24,"height":5}]}"#).map_err(|e|e.to_string())?;
    Ok(AcousticScene::build(&level))
}
pub fn run(audio: &mut AudioSystem, assets: &Path, out: &Path) -> Result<(), String> {
    let catalog = AudioCatalog::load(assets)?;
    let cues: Vec<_> = catalog
        .cue_ids()
        .filter(|id| id.starts_with("tg_"))
        .collect();
    if cues.len() != 23 {
        return Err("Use the isolated exact23original-cue validation bank".into());
    }
    let initial = audio.stats();
    if initial.decoded_bytes > places::engine::audio::MAX_DECODED_BYTES {
        return Err("Original cue bank exceeds unchanged64MiB cap".into());
    }
    let started = Instant::now();
    let mut capture = Capture {
        audio,
        out,
        scratch: vec![[0.0_f32; 2]; 8192],
    };
    let mut results = Vec::new();
    for mode in [SpatialMode::Speakers, SpatialMode::Headphones] {
        for cue in &cues {
            for scene in [Scene::Front, Scene::Rear, Scene::Closed, Scene::Muted] {
                let result = capture.trial(cue, mode, scene)?;
                std::fs::write(
                    out.join(format!("{mode:?}-{}-{cue}.json", scene.label())),
                    serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                results.push(result);
            }
        }
        results.push(capture.trial("tg_pursuit_shriek_loop", mode, Scene::Loop)?);
    }
    capture.audio.clear_world();
    let settled = capture.audio.stats();
    capture.audio.unload();
    let receipt = json!({"scope":"Actual native SDL/Kira device and internal stereo48kfloat PCM. All23frozenfinalcues sequentially; referencecomposites are isolated and never layered with runtimestems. Headphones means HRTFmix mode on the observed output device; no physicalheadphone/humanlistening claim. Gameplay owner/motion/collision/sensory acceptance is separate.","seconds":started.elapsed().as_secs_f64(),"initial":initial,"settled":settled,"unloaded":capture.audio.stats(),"callback_allocations":super::CALLBACK_ALLOCS.load(std::sync::atomic::Ordering::Relaxed),"callback_deallocations":super::CALLBACK_FREES.load(std::sync::atomic::Ordering::Relaxed),"results":results});
    std::fs::write(
        out.join("original-cues.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

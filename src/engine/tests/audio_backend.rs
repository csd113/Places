//! Four device-free regressions protect untrusted allocation boundaries, a
//! cache/mono validation bypass, and listener/clock numerical behavior. Their
//! unique value is rejecting malformed source content before decoding and
//! preserving actual world-clock and stereo orientation contracts. Fixtures
//! contain four PCM samples; no shipped assets, full maps or audio devices are
//! required. The two-voice mock also protects the pinned Kira retirement-queue
//! abort by forcing contested resource boundaries while rendering continues.
//! Expected cost is a few filesystem operations and milliseconds.

use std::error::Error;
use std::sync::atomic::AtomicUsize;

use super::*;

#[test]
fn contested_kira_resource_boundary_defers_retirement_while_audio_keeps_advancing()
-> Result<(), Box<dyn Error>> {
    use kira::backend::mock::{MockBackend, MockBackendSettings};

    let gate = AtomicBool::new(false);
    let mut manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
        main_track_builder: MainTrackBuilder::new().sound_capacity(2),
        backend_settings: MockBackendSettings {
            sample_rate: 44_100,
        },
        ..AudioManagerSettings::default()
    })
    .map_err(|()| "mock audio initialization failed")?;
    let data = StaticSoundData::from_cursor(Cursor::new(tiny_wav(1)))?;
    let first = manager
        .play(data.clone())
        .map_err(|error| format!("{error:?}"))?;
    let second = manager
        .play(data.clone())
        .map_err(|error| format!("{error:?}"))?;
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    let guard = ResourceGate::try_acquire(&gate).ok_or("resource gate must begin available")?;
    assert!(
        ResourceGate::try_acquire(&gate).is_none(),
        "gameplay must not publish into a callback-owned retirement boundary"
    );
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    manager.backend_mut().process();
    assert!(
        first.state() == PlaybackState::Stopped && second.state() == PlaybackState::Stopped,
        "a contested command boundary must still render and advance existing voices"
    );
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    assert_eq!(
        manager.main_track().num_sounds(),
        2,
        "contested retirement must remain deferred rather than freeing slots before retired publication"
    );
    drop(guard);
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    assert_eq!(
        manager.main_track().num_sounds(),
        0,
        "uncontested callback must retire the complete full-capacity batch"
    );
    let replacement = manager.play(data).map_err(|error| format!("{error:?}"))?;
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    manager.backend_mut().process();
    resource_boundary(&gate, || manager.backend_mut().on_start_processing());
    assert!(
        replacement.state() == PlaybackState::Stopped && manager.main_track().num_sounds() == 0,
        "replacement retirement must complete without overflowing Kira's retired queue"
    );
    Ok(())
}

#[test]
fn malformed_manifest_and_wav_are_rejected_before_decode() {
    let unsafe_path = br#"{"format_version":1,"cues":[{"id":"bad","files":["../outside.wav"]}]}"#;
    let duplicate_id = br#"{"format_version":1,"cues":[{"id":"same","files":["a.wav"]},{"id":"same","files":["b.wav"]}]}"#;
    let invalid_gain =
        br#"{"format_version":1,"cues":[{"id":"bad","files":["a.wav"],"gain":1e100}]}"#;
    for bytes in [
        unsafe_path.as_slice(),
        duplicate_id.as_slice(),
        invalid_gain.as_slice(),
    ] {
        assert!(
            AudioCatalog::from_json(bytes).is_err(),
            "unsafe or unbounded cue metadata must fail before file access"
        );
    }
    let valid = tiny_wav(1);
    assert!(
        wav_info(&valid, true).is_ok(),
        "small valid mono PCM fixture must be recognized"
    );
    let mut malformed = valid.clone();
    if let Some(size) = malformed.get_mut(40..44) {
        size.copy_from_slice(&u32::MAX.to_le_bytes());
    }
    assert!(
        wav_info(&malformed, true).is_err(),
        "huge declared data must fail without allocating decoded frames"
    );
    let mut zero_block = valid.clone();
    if let Some(block) = zero_block.get_mut(32..34) {
        block.copy_from_slice(&0_u16.to_le_bytes());
    }
    assert!(
        wav_info(&zero_block, true).is_err(),
        "zero block alignment must fail without division by zero"
    );
    let mut invalid_rate = valid;
    if let Some(rate) = invalid_rate.get_mut(24..28) {
        rate.copy_from_slice(&u32::MAX.to_le_bytes());
    }
    assert!(
        wav_info(&invalid_rate, true).is_err(),
        "unbounded sample rate must fail before decode"
    );
}

#[test]
fn cached_ambient_stereo_cannot_bypass_spatial_mono_validation() -> Result<(), Box<dyn Error>> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "places-audio-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let fixture = AudioFixture(root);
    fs::create_dir_all(fixture.0.join("audio"))?;
    fs::write(fixture.0.join("audio/stereo.wav"), tiny_wav(2))?;
    let catalog = AudioCatalog::from_json(br#"{"format_version":1,"cues":[{"id":"ambient","files":["stereo.wav"],"spatial":false},{"id":"spatial","files":["stereo.wav"],"spatial":true}]}"#)?;
    let result = load_clips(&fixture.0, catalog);
    assert!(
        result.err().is_some_and(|error| error.contains("mono")),
        "a cached stereo clip must be rechecked when a spatial cue reuses it"
    );
    let allowed = AudioCatalog::from_json(
        br#"{"format_version":1,"cues":[{"id":"ambient","files":["stereo.wav"],"spatial":false}]}"#,
    )?;
    assert!(
        load_clips(&fixture.0, allowed).is_ok(),
        "stereo ambience must remain supported"
    );
    Ok(())
}

#[test]
fn listener_orientation_and_finite_world_clock_preserve_mix_contract() {
    let settings = AudioSettings {
        spatial_mode: SpatialMode::Speakers,
        master_volume: 1.0,
        effects_volume: 1.0,
        ambient_volume: 1.0,
        muted: false,
    };
    let mix = Mix {
        settings,
        ..Mix::default()
    };
    let (near_gain, right_pan) =
        spatial_mix(mix, Some(Vec3::X), true, 1.0, 10.0, 1.0, AudioBus::Effects);
    assert!(
        (right_pan - 1.0).abs() < f32::EPSILON && (near_gain - 1.0).abs() < f32::EPSILON,
        "right ear direction must pan right at full near gain"
    );
    let listener = Listener {
        forward: Vec3::X,
        ..Listener::default()
    };
    let oriented = normalize_listener(listener);
    assert!(
        oriented.is_some(),
        "orthogonal finite listener must be accepted"
    );
    if let Some((normalized, right)) = oriented {
        let rotated = Mix {
            listener: normalized,
            right,
            ..mix
        };
        let (_, front_pan) = spatial_mix(
            rotated,
            Some(Vec3::X),
            true,
            1.0,
            10.0,
            1.0,
            AudioBus::Effects,
        );
        let (far_gain, _) = spatial_mix(
            rotated,
            Some(Vec3::new(0.0, 0.0, 10.0)),
            true,
            1.0,
            10.0,
            1.0,
            AudioBus::Effects,
        );
        assert!(
            front_pan.abs() < f32::EPSILON && far_gain.abs() < f32::EPSILON,
            "listener rotation must centre front sound and the maximum radius must reach silence"
        );
    }
    assert!(
        normalize_listener(Listener {
            forward: Vec3::Y,
            ..Listener::default()
        })
        .is_none(),
        "parallel forward/up directions must not enter the mixer"
    );
    let tiny = 0.001_f32;
    let slow = Mix {
        world_scale: sanitize_world_scale(tiny),
        ..mix
    };
    assert!(
        (playback_rate(slow, TimeDomain::World) - f64::from(tiny)).abs() < f64::EPSILON,
        "positive world time scales below .01 must retain exact speed"
    );
    let frozen = Mix {
        world_scale: sanitize_world_scale(f32::NAN),
        ..mix
    };
    assert!(
        world_paused(frozen, TimeDomain::World) && !world_paused(frozen, TimeDomain::Real),
        "nonfinite world clocks must safely pause world audio while real audio continues"
    );
    let menu = Mix {
        paused: true,
        ..mix
    };
    assert!(
        world_paused(menu, TimeDomain::World) && world_paused(menu, TimeDomain::Real),
        "menus and focus loss must pause both clock domains"
    );
    assert!(
        sanitize_gain(f32::INFINITY).abs() < f32::EPSILON,
        "nonfinite settings must become silence"
    );
}

struct AudioFixture(PathBuf);
impl Drop for AudioFixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn tiny_wav(channels: u16) -> Vec<u8> {
    let block = channels.saturating_mul(2);
    let data_bytes = u32::from(block).saturating_mul(2);
    let mut bytes = Vec::with_capacity(52);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&36_u32.saturating_add(data_bytes).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&44_100_u32.to_le_bytes());
    bytes.extend_from_slice(&44_100_u32.saturating_mul(u32::from(block)).to_le_bytes());
    bytes.extend_from_slice(&block.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_bytes.to_le_bytes());
    bytes.resize(
        44_usize.saturating_add(usize::try_from(data_bytes).unwrap_or_default()),
        0,
    );
    bytes
}

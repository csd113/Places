//! Protects tail ownership and post-effect master mute. This impulse fixture
//! processes four seconds with five fixed delay lines; no assets or devices.
use super::*;
#[test]
fn room_tail_contrast_mute_and_epoch_retirement_are_bounded() {
    let coefficients = crate::engine::audio::hrtf::kernel(glam::Vec3::X);
    let history: [f32; TAPS] =
        std::array::from_fn(|index| f32::from(u16::try_from(index).unwrap_or_default()).sin());
    let actual = convolve(&coefficients, &history);
    let expected_left = coefficients
        .iter()
        .zip(history.iter().rev())
        .map(|(tap, sample)| tap.left * sample)
        .sum::<f32>();
    let expected_right = coefficients
        .iter()
        .zip(history.iter().rev())
        .map(|(tap, sample)| tap.right * sample)
        .sum::<f32>();
    assert!(
        (actual.left - expected_left).abs() < 1.0e-5
            && (actual.right - expected_right).abs() < 1.0e-5,
        "SIMD must preserve all measured taps, ear ordering and history direction"
    );
    let render =
        |decay: f32, damping: f32| -> (RoomEffect, CommandWriter<OutputParameters>, Vec<f32>) {
            let (mut sender, receiver) = command_writer_and_reader();
            let room = RoomResponse {
                wet: 0.4,
                decay_seconds: decay,
                damping,
                size: [8.0, 3.0, 10.0],
            };
            sender.write(OutputParameters {
                room,
                gain: 1.0,
                epoch: 1,
            });
            let mut effect = RoomEffect::new(receiver);
            effect.begin();
            let mut warmup = vec![0.0_f32; 48_000];
            effect.process(&mut warmup);
            let mut samples = vec![0.0_f32; 192_000];
            if let Some(sample) = samples.first_mut() {
                *sample = 1.0;
            }
            effect.process(&mut samples);
            (effect, sender, samples)
        };
    let (_, _, office) = render(0.15, 0.8);
    let (mut effect, mut sender, pool) = render(2.5, 0.08);
    let tail_energy = |samples: &[f32]| {
        samples
            .get(48_000..)
            .unwrap_or_default()
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
    };
    assert!(
        tail_energy(&pool) > tail_energy(&office) * 10.0,
        "reflective room must retain a measurable late tail beyond the dry office"
    );
    for index in 0_u16..128 {
        sender.write(OutputParameters {
            gain: f32::from(index) / 128.0,
            epoch: 1,
            ..OutputParameters::default()
        });
    }
    sender.write(OutputParameters {
        gain: 0.0,
        epoch: 1,
        ..OutputParameters::default()
    });
    effect.begin();
    let mut muted = vec![0.0_f32; 24_000];
    effect.process(&mut muted);
    assert!(
        muted
            .get(12_000..)
            .unwrap_or_default()
            .iter()
            .all(|value| value.abs() < 1.0e-6),
        "latest master mute must survive rapid distinct controls and silence the existing effect tail"
    );
    sender.write(OutputParameters {
        gain: 1.0,
        epoch: 2,
        ..OutputParameters::default()
    });
    effect.begin();
    let mut cleared = vec![0.0_f32; 24_000];
    effect.process(&mut cleared);
    assert!(
        cleared.iter().all(|value| value.abs() < f32::EPSILON),
        "a new world epoch must retire every old delay sample"
    );
}

// A finite impulse at the very end of a block used to retire the wrapped sound
// before the measured FIR tail completed. This tiny no-device backend protects
// all 140 taps and prompt reclamation, at a cost of twelve 64-frame blocks.
#[test]
fn finite_source_preserves_complete_hrtf_tail_then_reclaims_its_slot() -> Result<(), String> {
    use kira::sound::static_sound::StaticSoundSettings;
    use kira::{AudioManager, AudioManagerSettings};
    let mut manager = AudioManager::<TailBackend>::new(AudioManagerSettings {
        internal_buffer_size: 64,
        ..AudioManagerSettings::default()
    })
    .map_err(|()| "tail backend setup")?;
    let mut frames = vec![Frame::ZERO; 128];
    *frames.last_mut().ok_or("impulse frame")? = Frame::from_mono(0.5);
    let data = StaticSoundData {
        sample_rate: 48_000,
        frames: frames.into(),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    let parameters = Parameters::new(
        &[Frame::from_mono(0.1); TAPS],
        true,
        AcousticPath::default(),
    );
    let (prepared, _sender) = spatial(data, &parameters, None);
    let _handle = manager
        .play(prepared)
        .map_err(|error| format!("{error:?}"))?;
    let mut output = [0.0_f32; 128];
    let mut audible = 0_usize;
    for _ in 0_u8..12 {
        manager.backend_mut().render(&mut output)?;
        audible = audible.saturating_add(
            output
                .as_chunks::<2>()
                .0
                .iter()
                .filter(|pair| pair[0].abs() > 0.01)
                .count(),
        );
    }
    assert!(
        audible >= TAPS,
        "a completed clip must keep every HRTF impulse tap instead of truncating at Kira's source boundary"
    );
    assert_eq!(
        manager.main_track().num_sounds(),
        0,
        "the completed tail must relinquish its bounded mixer slot"
    );
    Ok(())
}
struct TailBackend {
    renderer: Option<kira::backend::Renderer>,
}
impl kira::backend::Backend for TailBackend {
    type Settings = ();
    type Error = ();
    fn setup(_settings: (), _buffer_size: usize) -> Result<(Self, u32), ()> {
        Ok((Self { renderer: None }, 48_000))
    }
    fn start(&mut self, renderer: kira::backend::Renderer) -> Result<(), ()> {
        self.renderer = Some(renderer);
        Ok(())
    }
}
impl TailBackend {
    fn render(&mut self, output: &mut [f32]) -> Result<(), String> {
        let renderer = self.renderer.as_mut().ok_or("tail renderer")?;
        renderer.on_start_processing();
        renderer.process(output, 2);
        Ok(())
    }
}

// Unique coverage: an expired path must fade without another control message,
// a refreshed path must recover, and disposal must discard an in-flight tail.
// One constant 4-frame loop, 292 tiny blocks, no device/assets/files/map bake.
#[test]
fn stale_path_fades_recovers_and_world_disposal_discards_its_tail() -> Result<(), String> {
    use kira::sound::static_sound::StaticSoundSettings;
    use kira::{AudioManager, AudioManagerSettings};
    let mut manager = AudioManager::<TailBackend>::new(AudioManagerSettings {
        internal_buffer_size: 128,
        ..AudioManagerSettings::default()
    })
    .map_err(|()| "tail backend setup")?;
    let epoch = Arc::new(AtomicU64::new(1));
    let data = StaticSoundData {
        sample_rate: 48_000,
        frames: vec![Frame::from_mono(0.5); 4].into(),
        settings: StaticSoundSettings::default(),
        slice: None,
    }
    .loop_region(..);
    let mut parameters = Parameters::new(
        &[Frame::from_mono(0.1); TAPS],
        false,
        AcousticPath::default(),
    );
    parameters.valid_until = Some(std::time::Instant::now());
    let (prepared, mut sender) = spatial(
        data,
        &parameters,
        Some(WorldEpoch {
            current: Arc::clone(&epoch),
            voice: 1,
        }),
    );
    let _handle = manager
        .play(prepared)
        .map_err(|error| format!("{error:?}"))?;
    let mut output = [0.0_f32; 256];
    for _ in 0_u16..256 {
        manager.backend_mut().render(&mut output)?;
    }
    assert!(
        output.iter().all(|sample| sample.to_bits() == 0),
        "expired geometry must reach digital silence without subnormal output or another main-thread control"
    );
    parameters.valid_until = None;
    sender.write(parameters);
    for _ in 0_u8..32 {
        manager.backend_mut().render(&mut output)?;
    }
    assert!(
        output.iter().any(|sample| sample.abs() > 0.1),
        "a current exact path must restore the existing voice without restarting it"
    );
    epoch.store(2, Ordering::Release);
    for _ in 0_u8..4 {
        manager.backend_mut().render(&mut output)?;
    }
    assert!(
        output.iter().all(|sample| sample.abs() < f32::EPSILON),
        "a disposed world must emit no old source/FIR/IIR tail"
    );
    assert_eq!(
        manager.main_track().num_sounds(),
        0,
        "disposed loop must retire its Kira slot without a gameplay update"
    );
    Ok(())
}

// Unique onset regression: held 80 ms sources must remain exactly silent for
// six simulated query opportunities, then retain the entire clip and verified
// filter. This exercises 1/48 voice bursts without sleeps, devices or assets.
#[test]
fn unverified_short_sources_hold_their_cursor_until_bounded_admission() -> Result<(), String> {
    use kira::sound::static_sound::StaticSoundSettings;
    use kira::{AudioManager, AudioManagerSettings};
    for count in [1_usize, 48] {
        let mut energies = Vec::new();
        for transmission in [1.0_f32, 0.23] {
            let mut manager = AudioManager::<TailBackend>::new(AudioManagerSettings {
                internal_buffer_size: 128,
                ..AudioManagerSettings::default()
            })
            .map_err(|()| "admission backend setup")?;
            let mut writers = Vec::new();
            let data = StaticSoundData {
                sample_rate: 48_000,
                frames: vec![Frame::from_mono(0.01); 3840].into(),
                settings: StaticSoundSettings::default(),
                slice: None,
            };
            let mut parameters =
                Parameters::new(&[Frame::ZERO; TAPS], false, AcousticPath::default());
            parameters.admitted = false;
            parameters.transmission = 0.0;
            // Even an expired admission deadline cannot consume an unchecked clip.
            parameters.valid_until = Some(std::time::Instant::now());
            for _ in 0..count {
                let (prepared, writer) = spatial(data.clone(), &parameters, None);
                let _handle = manager.play(prepared).map_err(|e| format!("{e:?}"))?;
                writers.push(writer);
            }
            let mut output = [0.0_f32; 256];
            for _ in 0_u8..64 {
                manager.backend_mut().render(&mut output)?;
                assert!(
                    output.iter().all(|v| v.to_bits() == 0),
                    "unverified source emitted before geometry admission"
                );
            }
            assert_eq!(
                manager.main_track().num_sounds(),
                count,
                "short clips must survive a wait longer than their duration"
            );
            parameters.admitted = true;
            parameters.valid_until = None;
            parameters.transmission = transmission;
            let mut energy = 0.0_f32;
            for batch in writers.chunks_mut(8) {
                for writer in batch {
                    writer.write(parameters);
                }
                for _ in 0_u8..10 {
                    manager.backend_mut().render(&mut output)?;
                    energy += output.iter().map(|v| v * v).sum::<f32>();
                }
            }
            for _ in 0_u8..80 {
                manager.backend_mut().render(&mut output)?;
                energy += output.iter().map(|v| v * v).sum::<f32>();
            }
            assert_eq!(
                manager.main_track().num_sounds(),
                0,
                "admitted one-shots must retire normally"
            );
            assert!(
                energy > 0.001,
                "clear-path admission must keep short cues useful"
            );
            energies.push(energy);
        }
        assert!(
            energies.get(1).copied().unwrap_or_default()
                < energies.first().copied().unwrap_or_default() * 0.1,
            "closed admission cannot reproduce the clear-path onset waveform"
        );
    }
    Ok(())
}

// Held sounds have no filter tail: explicit stop and world disposal must reclaim
// them even without a first query. Two 4-frame clips, four blocks, no device.
#[test]
fn unverified_voices_retire_on_stop_and_world_disposal() -> Result<(), String> {
    use kira::sound::static_sound::StaticSoundSettings;
    use kira::{AudioManager, AudioManagerSettings};
    let mut manager = AudioManager::<TailBackend>::new(AudioManagerSettings {
        internal_buffer_size: 128,
        ..AudioManagerSettings::default()
    })
    .map_err(|()| "admission stop backend setup")?;
    let data = StaticSoundData {
        sample_rate: 48_000,
        frames: vec![Frame::from_mono(0.5); 4].into(),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    let epoch = Arc::new(AtomicU64::new(1));
    let mut parameters = Parameters::new(&[Frame::ZERO; TAPS], false, AcousticPath::default());
    parameters.admitted = false;
    let (first, _first_writer) = spatial(data.clone(), &parameters, None);
    let (second, _second_writer) = spatial(
        data,
        &parameters,
        Some(WorldEpoch {
            current: Arc::clone(&epoch),
            voice: 1,
        }),
    );
    let mut handle = manager.play(first).map_err(|e| format!("{e:?}"))?;
    let _other = manager.play(second).map_err(|e| format!("{e:?}"))?;
    let mut output = [0.0_f32; 256];
    manager.backend_mut().render(&mut output)?;
    handle.stop(kira::Tween {
        duration: std::time::Duration::ZERO,
        ..kira::Tween::default()
    });
    epoch.store(2, Ordering::Release);
    for _ in 0_u8..4 {
        manager.backend_mut().render(&mut output)?;
    }
    assert!(output.iter().all(|v| v.to_bits() == 0));
    assert_eq!(
        manager.main_track().num_sounds(),
        0,
        "unverified retirement must not wait for source playback or a path query"
    );
    Ok(())
}

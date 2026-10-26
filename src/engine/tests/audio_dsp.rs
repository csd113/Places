//! Protects tail ownership and post-effect master mute. This impulse fixture
//! processes four seconds with five fixed delay lines; no assets or devices.
use super::*;
#[test]
fn room_tail_contrast_mute_and_epoch_retirement_are_bounded() -> Result<(), String> {
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
    let render = |decay: f32,
                  damping: f32|
     -> Result<(RoomEffect, Producer<OutputParameters>, Vec<f32>), String> {
        let (mut sender, receiver) = RingBuffer::new(4);
        let room = RoomResponse {
            wet: 0.4,
            decay_seconds: decay,
            damping,
            size: [8.0, 3.0, 10.0],
        };
        sender
            .push(OutputParameters {
                room,
                gain: 1.0,
                epoch: 1,
            })
            .map_err(|error| error.to_string())?;
        let mut effect = RoomEffect::new(receiver);
        effect.begin();
        let mut warmup = vec![0.0_f32; 48_000];
        effect.process(&mut warmup);
        let mut samples = vec![0.0_f32; 192_000];
        if let Some(sample) = samples.first_mut() {
            *sample = 1.0;
        }
        effect.process(&mut samples);
        Ok((effect, sender, samples))
    };
    let (_, _, office) = render(0.15, 0.8)?;
    let (mut effect, mut sender, pool) = render(2.5, 0.08)?;
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
    sender
        .push(OutputParameters {
            gain: 0.0,
            epoch: 1,
            ..OutputParameters::default()
        })
        .map_err(|error| error.to_string())?;
    effect.begin();
    let mut muted = vec![0.0_f32; 24_000];
    effect.process(&mut muted);
    assert!(
        muted
            .get(12_000..)
            .unwrap_or_default()
            .iter()
            .all(|value| value.abs() < 1.0e-6),
        "master mute must also silence the existing effect tail"
    );
    sender
        .push(OutputParameters {
            gain: 1.0,
            epoch: 2,
            ..OutputParameters::default()
        })
        .map_err(|error| error.to_string())?;
    effect.begin();
    let mut cleared = vec![0.0_f32; 24_000];
    effect.process(&mut cleared);
    assert!(
        cleared.iter().all(|value| value.abs() < f32::EPSILON),
        "a new world epoch must retire every old delay sample"
    );
    Ok(())
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
    let (prepared, _sender) = spatial(data, &parameters);
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

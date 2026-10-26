//! Fixed storage DSP over Kira's existing preloaded sound and handle lifecycle.
use super::acoustics::{AcousticPath, RoomResponse};
use super::hrtf::{Kernel, TAPS};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::sound::{Sound, SoundData};
use kira::{Frame, info::Info};
use rtrb::{Consumer, Producer, RingBuffer};

#[derive(Clone, Copy)]
pub(super) struct Parameters {
    pub kernel: Kernel,
    pub binaural: f32,
    pub transmission: f32,
    pub lowpass: f32,
}
impl Parameters {
    pub fn new(kernel: &Kernel, binaural: bool, path: AcousticPath) -> Self {
        Self {
            kernel: *kernel,
            binaural: if binaural { 1.0 } else { 0.0 },
            transmission: path.transmission,
            lowpass: if path.cutoff_hz >= 20_000.0 {
                1.0
            } else {
                1.0 - (-std::f32::consts::TAU * path.cutoff_hz / 48_000.0).exp()
            },
        }
    }
}
pub(super) struct SpatialData {
    data: StaticSoundData,
    receiver: Consumer<Parameters>,
    initial: Parameters,
}
pub(super) fn spatial(
    data: StaticSoundData,
    initial: &Parameters,
) -> (SpatialData, Producer<Parameters>) {
    let (sender, receiver) = RingBuffer::new(2);
    (
        SpatialData {
            data,
            receiver,
            initial: *initial,
        },
        sender,
    )
}
impl SoundData for SpatialData {
    type Error = <StaticSoundData as SoundData>::Error;
    type Handle = StaticSoundHandle;
    fn into_sound(self) -> Result<(Box<dyn Sound>, Self::Handle), Self::Error> {
        let (inner, handle) = self.data.into_sound()?;
        Ok((
            Box::new(SpatialSound {
                inner,
                receiver: self.receiver,
                current: self.initial,
                target: self.initial,
                history: [0.0; TAPS * 2],
                cursor: 0,
                ramp: 0,
                silent_frames: 0,
                filtered: Frame::ZERO,
            }),
            handle,
        ))
    }
}
struct SpatialSound {
    inner: Box<dyn Sound>,
    receiver: Consumer<Parameters>,
    current: Parameters,
    target: Parameters,
    history: [f32; TAPS * 2],
    cursor: usize,
    ramp: u16,
    silent_frames: usize,
    filtered: Frame,
}
impl Sound for SpatialSound {
    fn on_start_processing(&mut self) {
        self.inner.on_start_processing();
        for _ in 0_u8..2 {
            if let Ok(value) = self.receiver.pop() {
                if self.target.kernel != value.kernel {
                    self.ramp = 576;
                }
                self.target = value;
            } else {
                break;
            }
        }
    }
    fn process(&mut self, out: &mut [Frame], dt: f64, info: &Info) {
        self.inner.process(out, dt, info);
        if out.iter().all(|frame| digital_zero(*frame)) {
            // Keep the complete FIR/IIR tail before skipping truly silent blocks.
            if self.silent_frames >= TAPS && digital_zero(self.filtered) {
                self.current = self.target;
                self.ramp = 0;
                self.silent_frames = self.silent_frames.saturating_add(out.len());
                return;
            }
            self.silent_frames = self.silent_frames.saturating_add(out.len());
        } else {
            self.silent_frames = 0;
        }
        for frame in out {
            const BLEND: f32 = 1.0 / 576.0;
            self.current.transmission = (self.target.transmission - self.current.transmission)
                .mul_add(BLEND, self.current.transmission);
            self.current.lowpass =
                (self.target.lowpass - self.current.lowpass).mul_add(BLEND, self.current.lowpass);
            self.current.binaural = (self.target.binaural - self.current.binaural)
                .mul_add(BLEND, self.current.binaural);
            self.filtered.left =
                (frame.left - self.filtered.left).mul_add(self.current.lowpass, self.filtered.left);
            self.filtered.right = (frame.right - self.filtered.right)
                .mul_add(self.current.lowpass, self.filtered.right);
            self.filtered.left = flush_silence(self.filtered.left);
            self.filtered.right = flush_silence(self.filtered.right);
            let mono = f32::midpoint(self.filtered.left, self.filtered.right);
            if let Some(slot) = self.history.get_mut(self.cursor) {
                *slot = mono;
            }
            if let Some(slot) = self.history.get_mut(self.cursor.saturating_add(TAPS)) {
                *slot = mono;
            }
            let convolution = if self.current.binaural > 0.000_01 || self.target.binaural > 0.0 {
                let history = self
                    .history
                    .get(
                        self.cursor.saturating_add(1)
                            ..self.cursor.saturating_add(TAPS).saturating_add(1),
                    )
                    .unwrap_or_default();
                if self.ramp > 0 {
                    blend_kernel(
                        &mut self.current.kernel,
                        &self.target.kernel,
                        1.0 / f32::from(self.ramp),
                    );
                    self.ramp = self.ramp.saturating_sub(1);
                    if self.ramp == 0 {
                        self.current.kernel = self.target.kernel;
                    }
                }
                convolve(&self.current.kernel, history)
            } else {
                Frame::ZERO
            };
            frame.left = (convolution.left - self.filtered.left)
                .mul_add(self.current.binaural, self.filtered.left)
                * self.current.transmission;
            frame.right = (convolution.right - self.filtered.right)
                .mul_add(self.current.binaural, self.filtered.right)
                * self.current.transmission;
            self.cursor = self
                .cursor
                .saturating_add(1)
                .checked_rem(TAPS)
                .unwrap_or_default();
        }
    }
    fn finished(&self) -> bool {
        self.inner.finished()
            && (self.silent_frames >= 2048
                || (self.silent_frames >= TAPS && digital_zero(self.filtered)))
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct OutputParameters {
    pub room: RoomResponse,
    pub gain: f32,
    pub epoch: u64,
}
const DELAY_SAMPLES: usize = 8192;
struct Comb {
    data: Box<[Frame]>,
    cursor: usize,
    filtered: Frame,
    delay: f32,
}
impl Comb {
    fn new() -> Self {
        Self {
            data: vec![Frame::ZERO; DELAY_SAMPLES].into_boxed_slice(),
            cursor: 0,
            filtered: Frame::ZERO,
            delay: 1601.0,
        }
    }
    fn clear(&mut self) {
        self.data.fill(Frame::ZERO);
        self.filtered = Frame::ZERO;
    }
    fn sample(&self, delay: f32) -> Frame {
        // Delay stays in 1..8190; integer conversion is bounded and cannot overflow.
        #[expect(
            clippy::as_conversions,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "finite clamped delay below the fixed buffer length"
        )]
        let whole = delay as usize;
        #[expect(
            clippy::as_conversions,
            clippy::cast_precision_loss,
            reason = "delay index is at most 8190, exactly representable as f32"
        )]
        let fraction = delay - whole as f32;
        let index = self
            .cursor
            .saturating_add(DELAY_SAMPLES)
            .saturating_sub(whole)
            % DELAY_SAMPLES;
        let next = index.saturating_add(DELAY_SAMPLES).saturating_sub(1) % DELAY_SAMPLES;
        let a = self.data.get(index).copied().unwrap_or(Frame::ZERO);
        let b = self.data.get(next).copied().unwrap_or(Frame::ZERO);
        Frame::new(
            (b.left - a.left).mul_add(fraction, a.left),
            (b.right - a.right).mul_add(fraction, a.right),
        )
    }
    fn process(&mut self, input: Frame, delay: f32, feedback: f32, damping: f32) -> Frame {
        self.delay = (delay - self.delay).mul_add(0.000_06, self.delay);
        let delayed = self.sample(self.delay);
        self.filtered.left =
            (delayed.left - self.filtered.left).mul_add(damping, self.filtered.left);
        self.filtered.right =
            (delayed.right - self.filtered.right).mul_add(damping, self.filtered.right);
        self.filtered.left = flush_silence(self.filtered.left);
        self.filtered.right = flush_silence(self.filtered.right);
        if let Some(slot) = self.data.get_mut(self.cursor) {
            *slot = Frame::new(
                self.filtered.right.mul_add(feedback, input.left),
                self.filtered.left.mul_add(feedback, input.right),
            );
        }
        self.cursor = self.cursor.saturating_add(1) % DELAY_SAMPLES;
        delayed
    }
}
/// One fixed four-comb stereo room plus shape-dependent early taps for the mix.
/// The master gain follows *after* the effect, so mute and pause silence tails.
pub(super) struct RoomEffect {
    lines: [Comb; 4],
    early: Comb,
    receiver: Consumer<OutputParameters>,
    current: OutputParameters,
    target: OutputParameters,
    gain_ramp: u16,
}
impl RoomEffect {
    pub fn new(receiver: Consumer<OutputParameters>) -> Self {
        Self {
            lines: std::array::from_fn(|_| Comb::new()),
            early: Comb::new(),
            receiver,
            current: OutputParameters::default(),
            target: OutputParameters::default(),
            gain_ramp: 0,
        }
    }
    pub fn begin(&mut self) {
        for _ in 0_u8..4 {
            let Ok(value) = self.receiver.pop() else {
                break;
            };
            if value.epoch != self.target.epoch {
                for line in &mut self.lines {
                    line.clear();
                }
                self.early.clear();
            }
            if self.target.gain.to_bits() != value.gain.to_bits() {
                self.gain_ramp = 576;
            }
            self.target = value;
        }
    }
    pub fn process(&mut self, samples: &mut [f32]) {
        let shape = self.target.room.size;
        let delays = [
            shape.first().copied().unwrap_or(1.0),
            shape.get(1).copied().unwrap_or(1.0),
            shape.get(2).copied().unwrap_or(1.0),
            shape.iter().sum::<f32>() * 0.41,
        ]
        .map(|v| f32::mul_add(v, 180.0, 700.0).clamp(700.0, 7500.0));
        let decay = self.target.room.decay_seconds.max(0.12);
        let feedback = delays.map(|v| 10.0_f32.powf(-3.0 * v / (48_000.0 * decay)).min(0.97));
        for pair in samples.as_chunks_mut::<2>().0 {
            if self.gain_ramp > 0 {
                self.current.gain +=
                    (self.target.gain - self.current.gain) / f32::from(self.gain_ramp);
                self.gain_ramp = self.gain_ramp.saturating_sub(1);
                if self.gain_ramp == 0 {
                    self.current.gain = self.target.gain;
                }
            }
            self.current.room.wet += (self.target.room.wet - self.current.room.wet) / 16_800.0;
            self.current.room.damping +=
                (self.target.room.damping - self.current.room.damping) / 16_800.0;
            let input = Frame::new(pair[0], pair[1]);
            let mut wet = Frame::ZERO;
            for ((line, delay), gain) in self.lines.iter_mut().zip(delays).zip(feedback) {
                let reflected = line.process(
                    input,
                    delay,
                    gain,
                    (1.0 - self.current.room.damping).clamp(0.05, 0.95),
                );
                wet.left = f32::mul_add(reflected.left, 0.25, wet.left);
                wet.right = f32::mul_add(reflected.right, 0.25, wet.right);
            }
            if let Some(slot) = self.early.data.get_mut(self.early.cursor) {
                *slot = input;
            }
            for delay in delays.into_iter().take(3) {
                let early = self.early.sample(delay * 0.45);
                wet.left = early.left.mul_add(0.16, wet.left);
                wet.right = early.right.mul_add(0.16, wet.right);
            }
            self.early.cursor = self.early.cursor.saturating_add(1) % DELAY_SAMPLES;
            pair[0] = wet.left.mul_add(self.current.room.wet, input.left) * self.current.gain;
            pair[1] = wet.right.mul_add(self.current.room.wet, input.right) * self.current.gain;
        }
    }
}

#[cfg(test)]
#[path = "../tests/audio_dsp.rs"]
mod tests;

fn flush_silence(value: f32) -> f32 {
    if value.abs() < 1.0e-20 { 0.0 } else { value }
}

// Explicit vector multiply/add avoids scalar fused-operation fallbacks on
// portable targets without FMA. Complete taps/sample rate are retained.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "glam Vec4 multiply/add operate only on floats; they have no integer-overflow or panic path"
)]
fn convolve(kernel: &Kernel, history: &[f32]) -> Frame {
    let mut left = glam::Vec4::ZERO;
    let mut right = glam::Vec4::ZERO;
    for (taps, samples) in kernel
        .as_chunks::<4>()
        .0
        .iter()
        .zip(history.as_chunks::<4>().0.iter().rev())
    {
        let input = glam::Vec4::new(samples[3], samples[2], samples[1], samples[0]);
        left += input * glam::Vec4::new(taps[0].left, taps[1].left, taps[2].left, taps[3].left);
        right +=
            input * glam::Vec4::new(taps[0].right, taps[1].right, taps[2].right, taps[3].right);
    }
    Frame::new(left.element_sum(), right.element_sum())
}
fn blend_kernel(current: &mut Kernel, target: &Kernel, blend: f32) {
    for (previous, next) in current
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(target.as_chunks::<2>().0)
    {
        let from = glam::Vec4::new(
            previous[0].left,
            previous[0].right,
            previous[1].left,
            previous[1].right,
        );
        let to = glam::Vec4::new(next[0].left, next[0].right, next[1].left, next[1].right);
        let [a, b, c, d] = from.lerp(to, blend).to_array();
        previous[0] = Frame::new(a, b);
        previous[1] = Frame::new(c, d);
    }
}

const fn digital_zero(frame: Frame) -> bool {
    frame.left.abs().to_bits() == 0 && frame.right.abs().to_bits() == 0
}

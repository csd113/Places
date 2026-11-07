#!/usr/bin/env python3
"""Original single-pitch Places ambience. Offline authoring only, no recordings.

All stochastic material is seeded, Fourier-bandlimited and periodic. Every
carrier and modulator closes on the 16 second period; room delays are circular.
No fade-to-zero or pasted end splice is used. Source reference is analyzed only,
not sampled into these originals. Copyright 2026 Places contributors, MIT.
"""
from __future__ import annotations
import io
import wave
import numpy as np


MASTER_RATE = 48_000
SECONDS = 16
CONFIGS = [
    dict(id='threshold_air', title='Threshold Air', hz=196.0, seed=72101,
         phase=0.23, pm=[0.27, 0.11, 0.035],
         envelope=[0.41, 0.22, 0.09, 0.035], env_phase=[-0.9, 1.1, 0.4],
         partials=[[2.0, 0.050, 0.8], [3.0, 0.012, 1.4]],
         shadow_offset=-0.125, shadow_gain=0.025,
         air_center=410.0, air_width=170.0, air_gain=0.005,
         room_delays=[[0.071, 0.12], [0.193, 0.08], [0.327, 0.04]],
         rms=0.034,
         character='A warm, almost pure G3 held in a dark doorway; slow pressure changes, no melody.'),
    dict(id='pale_signal', title='Pale Signal', hz=246.94, seed=72102,
         phase=0.81, pm=[0.12, 0.08, 0.025],
         envelope=[0.40, 0.23, 0.075, 0.025], env_phase=[0.9, -0.6, 1.7],
         partials=[[2.0, 0.082, 2.4], [3.0, 0.008, 0.3]],
         shadow_offset=0.1875, shadow_gain=0.032,
         air_center=990.0, air_width=220.0, air_gain=0.0035,
         room_delays=[[0.113, 0.10], [0.239, -0.055], [0.419, 0.04]],
         rms=0.034,
         character='A cold B3 glint with a faint octave halo, gently hovering rather than rising to a sting.'),
    dict(id='below_the_landing', title='Below the Landing', hz=164.81, seed=72103,
         phase=1.71, pm=[0.35, 0.055, 0.045],
         envelope=[0.42, 0.205, 0.085, 0.045], env_phase=[2.1, -0.4, 0.75],
         partials=[[2.0, 0.025, 1.1], [2.985, 0.018, 2.0]],
         shadow_offset=-0.25, shadow_gain=0.028,
         air_center=275.0, air_width=105.0, air_gain=0.006,
         room_delays=[[0.097, 0.13], [0.281, 0.075], [0.503, 0.035]],
         rms=0.034,
         character='A hollow E3 with a barely inharmonic upper edge and very distant breath; restrained unease.'),
]

def periodic_resample(x, length):
    """Periodic real FFT resampling, including even-length Nyquist handling."""
    spectrum = np.fft.rfft(x)
    common = min(len(x), length)
    output = np.zeros(length // 2 + 1, dtype=complex)
    output[:common // 2 + 1] = spectrum[:common // 2 + 1]
    if common % 2 == 0 and len(x) != length:
        output[common // 2] *= 2 if length < len(x) else 0.5
    return np.fft.irfft(output, length) * (length / len(x))


def circular_air(n, fs, rng, center, width):
    f = np.fft.rfftfreq(n, 1 / fs)
    z = rng.normal(size=len(f)) + 1j * rng.normal(size=len(f))
    shape = np.exp(-0.5 * ((f - center) / width) ** 2)
    shape *= 1 - np.exp(-(f / 70) ** 4)
    shape[f > 2_500] = 0
    z *= shape
    z[0] = z[-1] = 0
    y = np.fft.irfft(z, n)
    return y / np.sqrt(np.mean(y * y))

def synth(c):
    n = MASTER_RATE * SECONDS
    t = np.arange(n, dtype=float) / MASTER_RATE
    theta = 2 * np.pi * t / SECONDS
    f0 = round(c['hz'] * SECONDS) / SECONDS
    pm = sum(a * np.sin((i + 1) * theta + [0.4, 1.7, -0.6][i]) for i, a in enumerate(c['pm']))
    phase = 2 * np.pi * f0 * t + c['phase'] + pm
    env = c['envelope'][0] + sum(a * np.sin((i + 1) * theta + c['env_phase'][i]) for i, a in enumerate(c['envelope'][1:]))
    assert np.min(env) > 0.035
    y = np.sin(phase)
    for ratio, gain, p in c['partials']:
        f = round(f0 * ratio * SECONDS) / SECONDS
        y += gain * np.sin(2 * np.pi * f * t + p + pm * 0.6)
    y += c['shadow_gain'] * np.sin(2 * np.pi * (f0 + c['shadow_offset']) * t + 1.2 + pm * 0.2)
    y *= env
    rng = np.random.default_rng(c['seed'])
    y += c['air_gain'] * circular_air(n, MASTER_RATE, rng, c['air_center'], c['air_width']) * (0.7 + 0.22 * np.sin(theta - 1.1))
    wet = sum(g * np.roll(y, round(delay * MASTER_RATE)) for delay, g in c['room_delays'])
    y += wet
    y -= np.mean(y)
    y *= c['rms'] / np.sqrt(np.mean(y * y))
    assert np.max(np.abs(y)) < 0.18
    # Rotate only, never repair endpoints. Place the wrap at a natural flat
    # local extremum near the quiet part of this analytic periodic waveform.
    compact = periodic_resample(y, 16_000 * SECONDS)
    d = compact - np.roll(compact, 1)
    dd = np.roll(compact, -1) - 2 * compact + np.roll(compact, 1)
    score = np.abs(d) + 0.1 * np.abs(dd)
    score[np.abs(compact) > np.quantile(np.abs(compact), 0.35)] = np.inf
    index = int(np.argmin(score)) * 3
    y = np.roll(y, -index)
    return y, dict(actual_fundamental_hz=f0, natural_seam_rotation_master_frames=index,
                   analytic_period_seconds=SECONDS, sample_use='No reference samples; original seeded synthesis only.')

def render(name, sample_rate=16000):
    config = next((c for c in CONFIGS if c['id'] == name), None)
    if config is None:
        raise ValueError(f"unknown original tonal ambience: {name}")
    if sample_rate not in (16000, 44100, 48000):
        raise ValueError("tonal authoring supports explicit 16000/44100/48000 rates")
    values, _ = synth(config)
    values = periodic_resample(values, sample_rate * SECONDS)
    return pcm16(values, sample_rate, config['seed'] + sample_rate + 16)


def pcm16(values, sample_rate, seed):
    rng = np.random.default_rng(seed)
    dither = (rng.random(len(values)) - rng.random(len(values))) / 32768
    pcm = np.rint((values + dither) * 32768).astype('<i2')
    result = io.BytesIO()
    with wave.open(result, 'wb') as wav:
        wav.setparams((1, 2, sample_rate, 0, 'NONE', 'not compressed'))
        wav.writeframes(pcm.tobytes())
    return result.getvalue()


def resample_reference(data, sample_rate, seed):
    with wave.open(io.BytesIO(data), 'rb') as wav:
        if (wav.getnchannels(), wav.getsampwidth()) != (1, 2):
            raise ValueError("periodic original resampling requires mono PCM16")
        rate = wav.getframerate()
        values = np.frombuffer(wav.readframes(wav.getnframes()), '<i2').astype(float) / 32768
    return pcm16(periodic_resample(values, round(len(values) * sample_rate / rate)), sample_rate, seed)


def render_master(name):
    """Rebuild the preserved 48 kHz PCM24 source master (not a runtime WAV)."""
    config = next((c for c in CONFIGS if c['id'] == name), None)
    if config is None:
        raise ValueError(f"unknown original tonal ambience: {name}")
    values, _ = synth(config)
    rng = np.random.default_rng(config['seed'] + MASTER_RATE + 24)
    q = np.rint(values * 8388608 + rng.random(len(values)) - rng.random(len(values))).astype(np.int32).astype(np.uint32)
    raw = np.column_stack([q & 255, (q >> 8) & 255, (q >> 16) & 255]).astype(np.uint8).tobytes()
    result = io.BytesIO()
    with wave.open(result, 'wb') as wav:
        wav.setparams((1, 3, MASTER_RATE, 0, 'NONE', 'not compressed'))
        wav.writeframes(raw)
    return result.getvalue()

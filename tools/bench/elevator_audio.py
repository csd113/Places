#!/usr/bin/env python3
"""Check actual native PCM for bounded elevator motor cues, outside CI.

Requires NumPy and FFmpeg. Reads recordings only; never builds, starts a device,
or substitutes authored WAVs for native output. Correlation demonstrates cue
identity and timing, not subjective sound quality or calibrated speaker level.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

import numpy as np

RATE = 24000
ROOT = Path(__file__).resolve().parents[2]


def decode(path):
    pcm = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(path),
                                   '-ac', '1', '-ar', str(RATE), '-f', 'f32le', '-'])
    return np.frombuffer(pcm, dtype='<f4').astype(np.float64)


def matches(samples, cue):
    assert len(samples) >= len(cue), 'Recording is shorter than one ride'
    size = 1 << (len(samples) + len(cue) - 1).bit_length()
    product = np.fft.rfft(samples, size) * np.fft.rfft(cue[::-1], size)
    correlation = np.fft.irfft(product, size)[len(cue)-1:len(samples)]
    energy = np.r_[0., np.cumsum(samples*samples)]
    windows = energy[len(cue):] - energy[:-len(cue)]
    cue_energy = float(np.dot(cue, cue))
    normalized = np.where(windows > len(cue)*.001**2,
                          correlation/np.sqrt(np.maximum(windows*cue_energy, 1e-30)), 0.)
    found = []
    while float(np.max(normalized)) >= .9:
        index = int(np.argmax(normalized))
        found.append(dict(start_seconds=index/RATE, seconds=len(cue)/RATE,
                          correlation=float(normalized[index]),
                          linear_gain=float(correlation[index]/cue_energy),
                          rms=float(np.sqrt(max(windows[index], 0.)/len(cue)))))
        normalized[max(0, index-len(cue)):index+len(cue)] = 0.
    return sorted(found, key=lambda entry: entry['start_seconds'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wav', type=Path, required=True)
    parser.add_argument('--cue', type=Path, default=ROOT/'assets/audio/elevator_travel/01.wav')
    parser.add_argument('--expected-rides', type=int, required=True)
    idle_group = parser.add_mutually_exclusive_group()
    idle_group.add_argument('--idle-after', type=float,
                            help='Start of a known silent tail, in absolute native audio seconds')
    idle_group.add_argument('--idle-after-last-ride', type=float,
                            help='Start of a known silent tail, seconds after the final motor cue ends')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    samples, cue = decode(args.wav), decode(args.cue)
    rides = matches(samples, cue)
    idle_after = args.idle_after
    if args.idle_after_last_ride is not None:
        assert rides, 'A cue-relative tail requires a matched ride'
        idle_after = rides[-1]['start_seconds'] + rides[-1]['seconds'] + args.idle_after_last_ride
    report = dict(recording_sha256=hashlib.sha256(args.wav.read_bytes()).hexdigest(),
                  cue_sha256=hashlib.sha256(args.cue.read_bytes()).hexdigest(),
                  recording_seconds=len(samples)/RATE, rides=rides,
                  recording_peak=float(np.max(np.abs(samples))),
                  scope='Actual native PCM waveform identity and timing; no listening or SPL claim')
    if idle_after is not None:
        idle = samples[round(idle_after*RATE):]
        assert len(idle) >= RATE//2, 'Silent-tail witness must be at least half a second'
        report['idle_rms'] = float(np.sqrt(np.mean(idle*idle)))
        report['idle_after_seconds'] = idle_after
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2)+'\n')
    assert len(rides) == args.expected_rides, (len(rides), args.expected_rides)
    assert report['recording_peak'] < 1, 'Native recording clips'
    if idle_after is not None:
        assert report['idle_rms'] < .00001, report['idle_rms']
    print(f'{len(rides)} native motor cues, {len(cue)/RATE:.2f} s each; waveform and timing verified')


if __name__ == '__main__':
    main()

"""Original Feltling voice candidates, rendered offline to mono PCM16 WAV.

Copyright 2026 Places contributors; MIT, matching the Places repository.
No recordings, external voices, speech models or runtime synthesis are used.
The explicit syllable scores below are the editable source. Native playback,
mix acceptance and human listening remain separate from this offline audit.
"""

from array import array
import hashlib
import io
import argparse
import json
import math
from pathlib import Path
import random
import struct
import sys
import wave

RATE = 44_100
PEAK = 0.38

# (start seconds, length seconds, start/end fundamental Hz, roundness).
# Intent is gentle nonverbal greeting/conversation, a rising invitation,
# and a bright double-hop cheer. Every phrase has a natural silent tail.
SCORES = {
    "feltling_babble": (0.54, ((0.025, 0.105, 410, 465, 0.8),
                               (0.155, 0.135, 535, 440, 0.4),
                               (0.325, 0.145, 390, 505, 0.95))),
    "feltling_attention": (0.66, ((0.025, 0.14, 430, 560, 0.8),
                                  (0.205, 0.115, 485, 530, 0.6),
                                  (0.365, 0.22, 440, 645, 0.9))),
    "feltling_cheer": (0.62, ((0.025, 0.21, 480, 630, 0.65),
                              (0.295, 0.245, 535, 720, 0.8))),
}


def render_pcm(name):
    seconds, syllables = SCORES[name]
    values = [0.0] * round(seconds * RATE)
    rng = random.Random(4801 + list(SCORES).index(name))
    for start, length, low, high, roundness in syllables:
        phase = 0.0
        filtered = 0.0
        for frame in range(round(length * RATE)):
            t = frame / RATE
            progress = t / length
            ease = progress * progress * (3 - 2 * progress)
            pitch = low + (high - low) * ease
            pitch *= 1 + 0.025 * math.sin(math.tau * 7.3 * t)
            phase += math.tau * pitch / RATE
            # Smooth voiced harmonics with moving rounded vowel colours;
            # neither sample imitation nor a claimed human syllable.
            formant = 820 + 420 * (1 - roundness) + 100 * math.sin(math.pi * progress)
            voice = 0.0
            for harmonic in range(1, 9):
                frequency = harmonic * pitch
                colour = 0.28 + math.exp(-((frequency - formant) / 420) ** 2)
                colour += 0.35 * math.exp(-((frequency - 2250) / 600) ** 2)
                voice += math.sin(harmonic * phase) * colour / harmonic ** 1.65
            noise = rng.uniform(-1, 1)
            filtered += 0.18 * (noise - filtered)
            envelope = math.sin(math.pi * progress) ** 1.5
            value = math.tanh(1.3 * voice + 0.018 * filtered) * envelope
            values[round(start * RATE) + frame] += value
    maximum = max(abs(value) for value in values)
    pcm = array("h", (round(value * PEAK / maximum * 32767) for value in values))
    if sys.byteorder != "little":
        pcm.byteswap()
    return pcm.tobytes()


def render(name):
    """Return one original encoded mono WAV for the normal asset importer."""
    output = io.BytesIO()
    with wave.open(output, "wb") as wav:
        wav.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
        wav.writeframes(render_pcm(name))
    return output.getvalue()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="Explicit new review output directory")
    args = parser.parse_args()
    root = out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    records = []
    cues = []
    total_decoded = 0
    for name in SCORES:
        pcm = render_pcm(name)
        path = out / f"{name}.wav"
        with wave.open(str(path), "wb") as wav:
            wav.setparams((1, 2, RATE, 0, "NONE", "not compressed"))
            wav.writeframes(pcm)
        data = path.read_bytes()
        values = struct.unpack(f"<{len(pcm) // 2}h", pcm)
        rms = math.sqrt(sum((value / 32768) ** 2 for value in values) / len(values))
        decoded = len(values) * 8
        total_decoded += decoded
        records.append({"cue": name, "file": path.name, "sha256": hashlib.sha256(data).hexdigest(),
                        "encoded_bytes": len(data), "frames": len(values),
                        "duration_seconds": len(values) / RATE, "decoded_stereo_float_bytes": decoded,
                        "peak": max(abs(value) for value in values) / 32768,
                        "rms": rms, "first_sample": values[0], "last_sample": values[-1],
                        "leading_silence_frames": next(i for i, value in enumerate(values) if value),
                        "trailing_silence_frames": next(i for i, value in enumerate(reversed(values)) if value),
                        "max_adjacent_delta": max(abs(a - b) for a, b in zip(values, values[1:])) / 32768,
                        "looping": False})
        cues.append({"id": name, "files": [f"feltling/{path.name}"], "gain": 0.28,
                     "looping": False, "spatial": True, "min_distance": 1,
                     "max_distance": 28 if name == "feltling_attention" else 14,
                     "rms_amplitude": rms})
    report = {"status": "OFFLINE_CANDIDATES_ONLY", "license": "MIT", "sample_rate": RATE,
              "channels": 1, "bits_per_sample": 16, "source": "feltling.py; explicit additive/formant scores",
              "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "existing_decoded_bytes_at_3ffe6e2": 66_221_488,
              "new_decoded_stereo_float_bytes": total_decoded,
              "combined_decoded_stereo_float_bytes": 66_221_488 + total_decoded,
              "library_cap_bytes": 64 * 1024 * 1024,
              "remaining_bytes": 64 * 1024 * 1024 - 66_221_488 - total_decoded,
              "files": records, "native_playback": "NOT RUN", "human_listening": "NOT PERFORMED",
              "state_wiring": "PENDING IMPLEMENTATION", "runtime_loop_seam": "NOT APPLICABLE; finite one-shots"}
    if report["remaining_bytes"] < 0:
        raise ValueError("Candidate bank exceeds the existing library budget")
    (root / "offline-audit.json").write_text(json.dumps(report, indent=2) + "\n")
    (root / "catalog-fragment.json").write_text(json.dumps({"cues": cues}, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()

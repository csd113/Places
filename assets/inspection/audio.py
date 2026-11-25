"""Pure WAV identity, format, and decoded-bank budget checks."""
import argparse
from array import array
import hashlib
import io
import json
import math
from pathlib import Path, PurePosixPath
import re
import sys
import wave

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / "assets/audio"

RATE = 44_100
MAX_DECODED_BYTES = 64 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def clip_rate(clip):
    rate = clip.get("sample_rate", RATE)
    if type(rate) is not int or not 8000 <= rate <= 48000:
        raise ValueError(f"invalid explicit audio sample rate: {clip['file']}")
    return rate


def wav_record(data, clip, original):
    if len(data) > 16 * 1024 * 1024:
        raise ValueError(f"WAV exceeds 16 MiB: {clip['file']}")
    if len(data) < 12 or data[:4] != b"RIFF" or data[8:12] != b"WAVE" or int.from_bytes(data[4:8], "little") + 8 != len(data):
        raise ValueError(f"WAV RIFF size mismatch: {clip['file']}")
    with wave.open(io.BytesIO(data), "rb") as wav:
        channels, bits, rate, frames = (wav.getnchannels(), wav.getsampwidth() * 8,
                                      wav.getframerate(), wav.getnframes())
        if (channels, bits, rate) != (1, 16, clip_rate(clip)) or not 0 < frames <= 30 * rate:
            raise ValueError(f"WAV explicit format/duration mismatch: {clip['file']}")
        if len(wav.readframes(frames)) != frames * channels * 2:
            raise ValueError(f"WAV truncated sample data: {clip['file']}")
    if clip.get("expected_sha256", digest(data)) != digest(data):
        raise ValueError(f"pinned rendered WAV mismatch: {clip['file']}")
    return dict(file=clip["file"], source=clip["source"], source_file=clip["source_file"],
                source_file_sha256=digest(original), sha256=digest(data), frames=frames,
                sample_rate=rate, channels=channels, sample_bits=bits)


def preflight_bank(records):
    if len({r["file"] for r in records}) != len(records) or len(records) > 128:
        raise ValueError("duplicate or over-budget audio bank WAV records")
    if sum(r["frames"] * 8 for r in records) > MAX_DECODED_BYTES:
        raise ValueError("runtime audio catalog decoded budget exceeded")


def relative_path(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or "\\" in value or any(
            part in ("", ".", "..") for part in value.split("/")):
        raise ValueError(f"unsafe audio-relative path: {value!r}")
    return path


def waveform_rms(name, audio_root=AUDIO):
    with wave.open(str(audio_root / name), "rb") as wav:
        values = array("h", wav.readframes(wav.getnframes()))
    if sys.byteorder != "little":
        values.byteswap()
    return math.sqrt(sum(value * value for value in values) / max(1, len(values))) / 32768


def audit(authoring, *, root=ROOT, audio_root=AUDIO):
    catalog = json.loads((audio_root / "catalog.json").read_text())
    provenance = json.loads((audio_root / "provenance.json").read_text())
    if catalog["format_version"] != 1 or provenance["format_version"] != 1:
        raise ValueError("unsupported audio manifest version")
    if provenance["sources"] != authoring["sources"]:
        raise ValueError("source provenance differs from authoring recipes")
    records = {record["file"]: record for record in provenance["files"]}
    clips = {clip["file"]: clip for clip in authoring["clips"]}
    if (len(records) != len(provenance["files"]) or len(clips) != len(authoring["clips"])
            or set(records) != set(clips)):
        raise ValueError("duplicate or missing WAV provenance")
    source_ids = {source["id"] for source in provenance["sources"]}
    cues = set()
    used = set()
    for cue in catalog["cues"]:
        if not re.fullmatch(r"[A-Za-z0-9_.:-]{1,64}", cue["id"]) or cue["id"] in cues:
            raise ValueError(f"invalid or duplicate cue: {cue['id']}")
        cues.add(cue["id"])
        if not math.isfinite(cue["gain"]) or not 0 <= cue["gain"] <= 1:
            raise ValueError(f"invalid cue gain: {cue['id']}")
        if not cue["files"] or len(cue["files"]) > 8:
            raise ValueError(f"cue needs 1..8 variants: {cue['id']}")
        for name in cue["files"]:
            relative_path(name)
            if name not in records:
                raise ValueError(f"untracked cue file: {name}")
            used.add(name)
        measured = max(waveform_rms(name, audio_root) for name in cue["files"])
        if abs(cue.get("rms_amplitude", -1) - measured) > 1e-6:
            raise ValueError(f"cue hearing RMS differs from WAVs: {cue['id']}")
    if used != set(records) or used != {p.relative_to(audio_root).as_posix() for p in audio_root.rglob("*.wav")}:
        raise ValueError("unreferenced WAV asset or provenance record")
    encoded_bytes = frames_total = 0
    for name, record in records.items():
        data = (audio_root / name).read_bytes()
        if digest(data) != record["sha256"] or record["source"] not in source_ids:
            raise ValueError(f"WAV hash/provenance mismatch: {name}")
        recipe = clips[name]
        if record["source"] != recipe["source"] or record["source_file"] != recipe["source_file"]:
            raise ValueError(f"WAV source recipe mismatch: {name}")
        if recipe.get("resample") and record["source_file_sha256"] != recipe["resample"].get("source_sha256"):
            raise ValueError(f"WAV original source hash differs from conversion recipe: {name}")
        checked = wav_record(data, recipe, data)
        if any(checked[key] != record[key] for key in ("sample_rate", "sample_bits", "channels", "frames")):
            raise ValueError(f"WAV declared format/provenance mismatch: {name}")
        with wave.open(io.BytesIO(data), "rb") as wav:
            if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) != (
                    1, 2, clip_rate(recipe), record["frames"]):
                raise ValueError(f"WAV format mismatch: {name}")
            values = array("h", wav.readframes(wav.getnframes()))
            if sys.byteorder != "little":
                values.byteswap()
        if not values or record["frames"] > 30 * clip_rate(recipe) or max(map(abs, values)) > 30_000:
            raise ValueError(f"WAV duration/signal budget exceeded: {name}")
        if max(map(abs, values)) < 100:
            raise ValueError(f"WAV contains no useful signal: {name}")
        if recipe.get("crossfade_seconds") or recipe.get("loop_seam_check"):
            # A noise recording has natural adjacent-sample changes too. Compare
            # the boundary to the recording's derivative, not an arbitrary zero.
            step_rms = math.sqrt(sum((b - a) ** 2 for a, b in zip(values, values[1:]))
                                 / max(1, len(values) - 1))
            if abs(values[-1] - values[0]) > max(512, step_rms * 8):
                raise ValueError(f"loop seam discontinuity: {name}")
        encoded_bytes += len(data)
        frames_total += record["frames"]
    if len(cues) > 64:
        raise ValueError("runtime audio catalog cue budget exceeded")
    preflight_bank(records.values())
    # Catch map typos without requiring any map bake or a decoder dependency.
    def inspect(value, source):
        if isinstance(value, dict):
            if (value.get("component") == "audio" or value.get("action") == "play_sound" or ("sound" in value and "spatial" in value)) and "sound" in value:
                if value["sound"] not in cues:
                    raise ValueError(f"{source}: unknown sound {value['sound']}")
            for child in value.values():
                inspect(child, source)
        elif isinstance(value, list):
            for child in value:
                inspect(child, source)
    paths = list((root / "levels").glob("*.json"))
    paths += list((root / "tests/fixtures/native").glob("*.json"))
    paths += list((root / "projects").glob("*/levels/*.json"))
    for path in paths:
        inspect(json.loads(path.read_text()), path.relative_to(root))
    print(f"Audio audit: {len(cues)} cues, {len(records)} WAVs, {encoded_bytes:,} bytes; "
          f"{frames_total * 8 / 1048576:.2f} MiB decoded stereo float budget; provenance and map cues valid")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="audit the shipped WAVs and cue references (the default)")
    parser.parse_args()
    authoring = json.loads((AUDIO / "authoring.json").read_text())
    audit(authoring)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, wave.Error) as error:
        raise SystemExit(f"Audio audit failed: {error}") from error

#!/usr/bin/env python3
"""Audit shipped audio offline, or explicitly import the hash-pinned sources.

The game only loads the committed WAVs. Importing is an opt-in authoring operation,
not a build/startup step. Source archives stay in the selected external cache.
"""
from __future__ import annotations

import argparse
from array import array
import hashlib
import io
import json
import math
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import urllib.request
import wave
import zipfile

ROOT = Path(__file__).resolve().parents[2]
AUDIO = ROOT / "assets/audio"
RATE = 44_100
MAX_DECODED_BYTES = 64 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative_path(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or "\\" in value or any(
            part in ("", ".", "..") for part in value.split("/")):
        raise ValueError(f"unsafe audio-relative path: {value!r}")
    return path


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def source_bytes(source, clip, cache, download):
    path = cache / source["cache_file"]
    if not path.exists():
        if source.get("delivery") == "user_attachment":
            raise ValueError(f"missing attached source {path}; restore the supplied file, "
                             "not a site preview or replacement download")
        if not download:
            raise ValueError(f"missing {path}; pass --download to fetch pinned sources")
        with urllib.request.urlopen(source["download_url"], timeout=60) as response:
            data = response.read(32 * 1024 * 1024 + 1)
        if len(data) > 32 * 1024 * 1024:
            raise ValueError(f"source exceeds 32 MiB: {source['id']}")
        if digest(data) != source["sha256"]:
            raise ValueError(f"download hash changed for {source['id']}")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    data = path.read_bytes()
    if digest(data) != source["sha256"]:
        raise ValueError(f"source hash mismatch: {path}")
    if source.get("archive") == "zip":
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            member = archive.getinfo(clip["source_file"])
            if member.file_size > 16 * 1024 * 1024:
                raise ValueError(f"source member exceeds 16 MiB: {member.filename}")
            return archive.read(member)
    if clip["source_file"] != source["cache_file"]:
        raise ValueError(f"source filename mismatch: {clip['file']}")
    return data


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


def original_audio(clip):
    # Unknown source IDs must not accidentally dispatch to the legacy renderer.
    if clip["source"] == "places_original_elevator_0131":
        from elevator import render
    elif clip["source"] == "places_original_feltling_0131":
        from feltling import render
    elif clip["source"] == "places_original_ambient_013":
        from original import render
    elif clip["source"] == "places_original_tonal_ambient_0131":
        from tonal import render
        original = render(clip["source_file"], clip_rate(clip))
        return original, original
    else:
        raise ValueError(f"unknown original audio source: {clip['source']}")
    original = render(clip["source_file"])
    conversion = clip.get("resample")
    if conversion is None:
        return original, original
    if (conversion.get("method") != "periodic_fft_tpdf"
            or type(conversion.get("seed")) is not int
            or conversion["seed"] < 0
            or digest(original) != conversion.get("source_sha256")):
        raise ValueError(f"invalid or changed original resample source: {clip['file']}")
    from tonal import resample_reference
    return original, resample_reference(original, clip_rate(clip), conversion["seed"])


def preflight_bank(records):
    if len({r["file"] for r in records}) != len(records) or len(records) > 128:
        raise ValueError("duplicate or over-budget audio import")
    if sum(r["frames"] * 8 for r in records) > MAX_DECODED_BYTES:
        raise ValueError("runtime audio catalog decoded budget exceeded before publication")


def edit_pcm(data, clip):
    rate = clip_rate(clip)
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", "pipe:0", "-map", "0:a:0",
         "-f", "s16le", "-acodec", "pcm_s16le", "-ac", "1", "-ar", str(rate),
         "-fflags", "+bitexact", "-flags:a", "+bitexact", "pipe:1"],
        input=data, capture_output=True, check=True).stdout
    values = array("h")
    values.frombytes(raw)
    if sys.byteorder != "little":
        values.byteswap()
    if clip.get("edit_mode") == "format_only":
        if any(key in clip for key in ("start_seconds", "end_seconds", "crossfade_seconds", "peak", "post_peak_gain")):
            raise ValueError(f"format-only import cannot edit authored timing or gain: {clip['file']}")
        result = io.BytesIO()
        with wave.open(result, "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(rate)
            wav.writeframes(raw)
        return result.getvalue()
    if clip.get("edit_mode", "polish") != "polish":
        raise ValueError(f"unknown audio edit mode: {clip['file']}")
    start = round(clip.get("start_seconds", 0) * rate)
    stop = round(clip["end_seconds"] * rate) if "end_seconds" in clip else len(values)
    samples = list(values[start:stop])
    if not samples:
        raise ValueError(f"empty audio selection: {clip['file']}")
    # Remove source DC offset before the edge edits and peak normalization.
    dc = round(sum(samples) / len(samples))
    samples = [value - dc for value in samples]
    overlap = round(clip.get("crossfade_seconds", 0) * rate)
    if overlap:
        if overlap * 2 >= len(samples):
            raise ValueError(f"crossfade too long: {clip['file']}")
        blend = [round((samples[-overlap + i] * (overlap - 1 - i)
                        + samples[i] * i) / (overlap - 1)) for i in range(overlap)]
        samples = samples[overlap:-overlap] + blend
    else:
        # Keep a short pre-roll/tail, trim only near-silence, and feather cuts.
        threshold = max(8, round(max(abs(value) for value in samples) * .001))
        active = [i for i, value in enumerate(samples) if abs(value) > threshold]
        if not active:
            raise ValueError(f"silent audio selection: {clip['file']}")
        samples = samples[max(0, active[0] - 220):min(len(samples), active[-1] + 1324)]
        fade = min(132, len(samples) // 4)
        for i in range(fade):
            samples[i] = round(samples[i] * i / fade)
            samples[-1 - i] = round(samples[-1 - i] * i / fade)
    peak = max(abs(value) for value in samples)
    target = clip.get("peak", 26_214)
    if peak == 0 or not 0 < target < 32_767:
        raise ValueError(f"invalid peak: {clip['file']}")
    normalized = [round(value * target / peak) for value in samples]
    gain = clip.get("post_peak_gain", 1.0)
    if not 0 < gain <= 1:
        raise ValueError(f"invalid polish gain: {clip['file']}")
    pcm = array("h", (round(value * gain) for value in normalized))
    if sys.byteorder != "little":
        pcm.byteswap()
    result = io.BytesIO()
    with wave.open(result, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(pcm.tobytes())
    return result.getvalue()


def import_audio(authoring, cache, download, originals_only=False):
    sources = {source["id"]: source for source in authoring["sources"]}
    prior = {r["file"]: r for r in json.loads((AUDIO / "provenance.json").read_text())["files"]} if originals_only else {}
    records, staged = [], []
    for clip in authoring["clips"]:
        path = AUDIO / relative_path(clip["file"])
        if sources[clip["source"]].get("archive") == "original":
            original, encoded = original_audio(clip)
            record = wav_record(encoded, clip, original)
        elif originals_only:
            encoded = path.read_bytes()
            record = prior[clip["file"]]
            measured = wav_record(encoded, clip, encoded)
            if any(measured[key] != record[key] for key in
                   ("file", "source", "source_file", "sha256", "frames", "sample_rate", "channels", "sample_bits")):
                raise ValueError(f"retained source/provenance mismatch: {clip['file']}")
        else:
            original = source_bytes(sources[clip["source"]], clip, cache, download)
            encoded = edit_pcm(original, clip)
            record = wav_record(encoded, clip, original)
        records.append(record)
        staged.append((path, encoded))
    # No file or provenance is replaced until every render/hash/header and the
    # entire decoded bank pass. A missing rate override cannot publish 44.1 kHz.
    preflight_bank(records)
    version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
    for path, encoded in staged:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(encoded)
    write_json(AUDIO / "provenance.json", dict(format_version=1,
        imported_with=version, sources=authoring["sources"], files=records))



def waveform_rms(name):
    with wave.open(str(AUDIO / name), "rb") as wav:
        values = array("h", wav.readframes(wav.getnframes()))
    if sys.byteorder != "little":
        values.byteswap()
    return math.sqrt(sum(value * value for value in values) / max(1, len(values))) / 32768


def refresh_hearing_profiles():
    catalog = json.loads((AUDIO / "catalog.json").read_text())
    for cue in catalog["cues"]:
        cue["rms_amplitude"] = max(map(waveform_rms, cue["files"]))
    write_json(AUDIO / "catalog.json", catalog)


def audit(authoring):
    catalog = json.loads((AUDIO / "catalog.json").read_text())
    provenance = json.loads((AUDIO / "provenance.json").read_text())
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
        measured = max(map(waveform_rms, cue["files"]))
        if abs(cue.get("rms_amplitude", -1) - measured) > 1e-6:
            raise ValueError(f"cue hearing RMS differs from WAVs: {cue['id']}")
    if used != set(records) or used != {p.relative_to(AUDIO).as_posix() for p in AUDIO.rglob("*.wav")}:
        raise ValueError("unreferenced WAV asset or provenance record")
    encoded_bytes = frames_total = 0
    for name, record in records.items():
        data = (AUDIO / name).read_bytes()
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
    if len(cues) > 64 or len(records) > 128 or frames_total * 8 > MAX_DECODED_BYTES:
        raise ValueError("runtime audio catalog budget exceeded")
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
    paths = list((ROOT / "levels").glob("*.json"))
    paths += list((ROOT / "tests/fixtures/native").glob("*.json"))
    paths += list((ROOT / "projects").glob("*/levels/*.json"))
    for path in paths:
        inspect(json.loads(path.read_text()), path.relative_to(ROOT))
    print(f"Audio audit: {len(cues)} cues, {len(records)} WAVs, {encoded_bytes:,} bytes; "
          f"{frames_total * 8 / 1048576:.2f} MiB decoded stereo float budget; provenance and map cues valid")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="offline integrity audit (the default)")
    parser.add_argument("--import", dest="import_sources", action="store_true", help="rebuild committed WAVs")
    parser.add_argument("--originals-only", action="store_true", help="with --import, regenerate originals and verify/retain all imported recordings")
    parser.add_argument("--cache", type=Path, default=Path("/tmp/places-audio-sources"))
    parser.add_argument("--download", action="store_true", help="allow pinned upstream downloads during import")
    args = parser.parse_args()
    authoring = json.loads((AUDIO / "authoring.json").read_text())
    if args.download and not args.import_sources:
        parser.error("--download requires --import")
    if args.originals_only and not args.import_sources:
        parser.error("--originals-only requires --import")
    if args.originals_only and args.download:
        parser.error("--originals-only cannot download")
    if args.import_sources:
        import_audio(authoring, args.cache, args.download, args.originals_only)
        refresh_hearing_profiles()
    audit(authoring)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, wave.Error) as exc:
        raise SystemExit(f"Audio audit/import failed: {exc}") from exc

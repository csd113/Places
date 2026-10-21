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


def edit_pcm(data, clip):
    raw = subprocess.run(
        ["ffmpeg", "-v", "error", "-i", "pipe:0", "-map", "0:a:0",
         "-f", "s16le", "-acodec", "pcm_s16le", "-ac", "1", "-ar", str(RATE),
         "-fflags", "+bitexact", "-flags:a", "+bitexact", "pipe:1"],
        input=data, capture_output=True, check=True).stdout
    values = array("h")
    values.frombytes(raw)
    if sys.byteorder != "little":
        values.byteswap()
    start = round(clip.get("start_seconds", 0) * RATE)
    stop = round(clip["end_seconds"] * RATE) if "end_seconds" in clip else len(values)
    samples = list(values[start:stop])
    if not samples:
        raise ValueError(f"empty audio selection: {clip['file']}")
    # Remove source DC offset before the edge edits and peak normalization.
    dc = round(sum(samples) / len(samples))
    samples = [value - dc for value in samples]
    overlap = round(clip.get("crossfade_seconds", 0) * RATE)
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
        wav.setframerate(RATE)
        wav.writeframes(pcm.tobytes())
    return result.getvalue()


def import_audio(authoring, cache, download):
    sources = {source["id"]: source for source in authoring["sources"]}
    records = []
    for clip in authoring["clips"]:
        path = AUDIO / relative_path(clip["file"])
        original = source_bytes(sources[clip["source"]], clip, cache, download)
        encoded = edit_pcm(original, clip)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(encoded)
        with wave.open(io.BytesIO(encoded), "rb") as wav:
            frames = wav.getnframes()
        records.append(dict(file=clip["file"], source=clip["source"],
                            source_file=clip["source_file"],
                            source_file_sha256=digest(original), sha256=digest(encoded),
                            frames=frames, sample_rate=RATE, channels=1, sample_bits=16))
    version = subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0]
    write_json(AUDIO / "provenance.json", dict(format_version=1,
        imported_with=version, sources=authoring["sources"], files=records))


def audit(authoring):
    catalog = json.loads((AUDIO / "catalog.json").read_text())
    provenance = json.loads((AUDIO / "provenance.json").read_text())
    if catalog["format_version"] != 1 or provenance["format_version"] != 1:
        raise ValueError("unsupported audio manifest version")
    if provenance["sources"] != authoring["sources"]:
        raise ValueError("source provenance differs from authoring recipes")
    records = {record["file"]: record for record in provenance["files"]}
    clips = {clip["file"]: clip for clip in authoring["clips"]}
    if len(records) != len(provenance["files"]) or set(records) != set(clips):
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
    if used != set(records) or used != {str(p.relative_to(AUDIO)) for p in AUDIO.rglob("*.wav")}:
        raise ValueError("unreferenced WAV asset or provenance record")
    encoded_bytes = frames_total = 0
    for name, record in records.items():
        data = (AUDIO / name).read_bytes()
        if digest(data) != record["sha256"] or record["source"] not in source_ids:
            raise ValueError(f"WAV hash/provenance mismatch: {name}")
        recipe = clips[name]
        if record["source"] != recipe["source"] or record["source_file"] != recipe["source_file"]:
            raise ValueError(f"WAV source recipe mismatch: {name}")
        with wave.open(io.BytesIO(data), "rb") as wav:
            if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) != (
                    1, 2, RATE, record["frames"]):
                raise ValueError(f"WAV format mismatch: {name}")
            values = array("h", wav.readframes(wav.getnframes()))
            if sys.byteorder != "little":
                values.byteswap()
        if not values or record["frames"] > 30 * RATE or max(map(abs, values)) > 30_000:
            raise ValueError(f"WAV duration/signal budget exceeded: {name}")
        if max(map(abs, values)) < 100:
            raise ValueError(f"WAV contains no useful signal: {name}")
        if recipe.get("crossfade_seconds"):
            # A noise recording has natural adjacent-sample changes too. Compare
            # the boundary to the recording's derivative, not an arbitrary zero.
            step_rms = math.sqrt(sum((b - a) ** 2 for a, b in zip(values, values[1:]))
                                 / max(1, len(values) - 1))
            if abs(values[-1] - values[0]) > max(512, step_rms * 8):
                raise ValueError(f"loop seam discontinuity: {name}")
        encoded_bytes += len(data)
        frames_total += record["frames"]
    if len(cues) > 64 or len(records) > 128 or frames_total * 8 > 64 * 1024 * 1024:
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
    parser.add_argument("--cache", type=Path, default=Path("/tmp/places-audio-sources"))
    parser.add_argument("--download", action="store_true", help="allow pinned upstream downloads during import")
    args = parser.parse_args()
    authoring = json.loads((AUDIO / "authoring.json").read_text())
    if args.download and not args.import_sources:
        parser.error("--download requires --import")
    if args.import_sources:
        import_audio(authoring, args.cache, args.download)
    audit(authoring)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, wave.Error) as exc:
        raise SystemExit(f"Audio audit/import failed: {exc}") from exc

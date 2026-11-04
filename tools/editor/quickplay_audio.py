#!/usr/bin/env python3
"""Opt-in direct-editor audio acceptance. No package/lightmap bakes or user-profile writes.

Run serially after other Places native checks. Requires a real SDL audio device
and the installed asset root. Retains source, script, voice snapshots and real
post-mixer PCM; this does not claim physical speaker/headphone listening.
"""
import argparse
import array
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def step(kind, delay=100, **values):
    return {"delay_ms": delay, "action": {"kind": kind, **values}}


def checkpoint(name, delay=100):
    return step("checkpoint", delay, name=name)


def control(action):
    return step("control", action=action)


def key(name, pressed=None, delay=100):
    return step("key", delay, key=name, pressed=pressed)


def fixture(family, duplicates=1):
    level = {
        "schema": "places-level", "format_version": 1,
        "id": "quickplay_audio", "name": "Quick-play audio control",
        "spawn": {"x": 0, "z": 1.5, "yaw_degrees": 0},
        "rooms": [{"x": -8, "z": -4, "width": 48, "depth": 8, "height": 3.2}],
        "defaults": {"wall": "home:wall_paint_offwhite_01",
                     "floor": "home:hardwood_oak_01", "ceiling": "core:ceiling_panel_01"},
        "props": [], "sound_emitters": [],
    }
    if family in ("hvac", "all"):
        level["props"] += [
            {"id": f"duct_{i}", "model": "backrooms:hvac_duct_straight",
             "x": 0, "y": 2.3, "z": -1, "solid": False, "occludes": False}
            for i in range(duplicates)
        ]
        # This legacy emitter must never add a global or second duct voice.
        level["sound_emitters"].append({"id": "legacy_hvac", "sound": "ambience_hvac",
            "x": 0, "y": 1.6, "z": 0, "playing": True, "looped": True,
            "spatial": False, "gain": 0.8, "time_domain": "real"})
    if family == "all":
        level["ceiling_lights"] = [{"id": "hum_light", "fixture": "core:fluorescent_panel_01",
            "x": 0, "z": 0, "y": 3.1, "switchable": True, "align": "none"}]
        level["floor_regions"] = [{"x": -2, "z": -3.5, "width": 4, "depth": 1.5, "offset_y": -1}]
        level["water"] = [{"x": -2, "z": -3.5, "width": 4, "depth": 1.5,
                           "surface_y": -0.1, "bottom_y": -1}]
        level["doors"] = [{"id": "sound_door", "x": 2, "z": 0,
                            "width": 1, "height": 2.2, "open_speed_degrees": 120}]
        level["sound_emitters"].append({"id": "authored_water", "sound": "water_emitter",
            "x": -1.5, "y": 1.2, "z": 0, "playing": True, "looped": True,
            "spatial": True, "attenuation": [1, 9], "gain": 0.1})
        level["props"].append({"id": "audio_switch", "model": "home:wall_switch",
            "x": 0, "y": 1.2, "z": 0, "solid": False, "occludes": False,
            "components": [{"component": "interactable", "prompt": "Toggle audio controls", "reach": 2}],
            "bindings": [{"on": "interact", "actions": [
                {"action": "toggle", "target": "hum_light"},
                {"action": "toggle", "target": "sound_door"},
                {"action": "play_sound", "target": "authored_water", "loop": True},
                {"action": "play_sound", "target": "legacy_hvac", "loop": True}]}]})
    if family == "steps":
        level["floor_patches"] = [{"x": 3, "z": -4, "width": 6, "depth": 8,
                                   "material": "core:carpet_beige_01"}]
    return level


def play_camera(x=0, yaw=0, pitch=0):
    return [step("camera", eye=[x, 1.6, 1.5], yaw=yaw, pitch=pitch),
            control("PlayOptions"), control("PlayCamera"), step("wait_test")]


def stop(name="stopped"):
    return [key("Escape"), step("wait_idle"), checkpoint(name, 700)]


def actions(case, cue_index):
    steps = [step("resize", width=960, height=544), step("wait_idle"),
             step("focus"), checkpoint("editing", 500)]
    if case == "lifecycle":
        steps += play_camera(pitch=-0.2)
        steps += [checkpoint("near", 2200), step("capture", name="near"), key("E"),
                  checkpoint("door_opening", 180), checkpoint("lights_off", 2800),
                  key("E"), checkpoint("door_closing", 180), checkpoint("lights_on", 2200)]
        steps += [control("Stop"), step("wait_idle"), checkpoint("stopped", 700)]
        steps += play_camera(pitch=-0.2) + [checkpoint("restarted", 1800)]
        steps += [step("focus_lost"), checkpoint("focus_stopped", 700), step("wait_idle"), step("focus")]
        steps += [control("SceneSearch"), step("text", text="authored_water"),
                  control("Select(Audio(1))"), control("Advanced"), control("Gameplay"), control("GamePick(Sound)")]
        # The native chooser paginates ten installed cues at a time.
        steps += [control("Page(true)") for _ in range(cue_index // 10)]
        steps += [control(f"PreviewSound({cue_index})"), checkpoint("audition", 200),
                  checkpoint("audition_finished", 3200), key("Escape")]
    elif case == "hvac_move":
        steps += play_camera() + [checkpoint("near", 2000), key("D", True),
            key("D", False, delay=4200), checkpoint("walk_cutoff", 3000),
            key("A", True), key("A", False, delay=4200), checkpoint("returned", 2000)]
        steps += stop()
    elif case.startswith("hvac"):
        for label, x, yaw in [("near", 0, 0), ("left", -3, 0), ("right", 3, 0),
                              ("mid", 5, 0), ("cutoff", 15, 0)]:
            steps += play_camera(x, yaw) + [checkpoint(label, 2000)]
            steps += stop("stopped_" + label)
    elif case == "steps":
        steps += play_camera()
        steps += [checkpoint("idle", 600), key("D", True)]
        steps += [checkpoint(f"walk_{i}", 100) for i in range(24)]
        steps += [key("D", False), checkpoint("rest", 1000)]
        steps += stop()
    else:
        steps += play_camera() + [checkpoint("empty", 1800)] + stop()
    steps += [checkpoint("final", 300), step("close")]
    return steps


def read_snapshot(run, name):
    return json.loads((run / (name + ".json")).read_text())


def voices(snapshot, cue=None):
    found = snapshot["audio_voices"]
    return found if cue is None else [v for v in found if v["cue"] == cue]


def one(snapshot, cue):
    found = voices(snapshot, cue)
    assert len(found) == 1, (cue, found)
    return found[0]


def pcm(path):
    data = path.read_bytes()
    assert data[:4] == b"RIFF" and data[8:12] == b"WAVE", path
    assert struct.unpack_from("<HHI", data, 20) == (3, 2, 48000), path
    values = array.array("f")
    values.frombytes(data[44:])
    if sys.byteorder != "little":
        values.byteswap()
    return values


def rms_before(values, snapshot, seconds=0.5):
    end = snapshot["audio_capture_frames"]
    assert isinstance(end, int) and end > 24000, end
    first = max(0, end - int(seconds * 48000))
    samples = values[first * 2:end * 2]
    assert len(samples) == (end - first) * 2, "PCM interval missing"
    return [math.sqrt(sum(v * v for v in samples[channel::2]) / (end - first))
            for channel in (0, 1)]


def validate(case, run):
    snapshots = {p.stem: json.loads(p.read_text()) for p in run.glob("*.json")
                 if p.stem not in ("script", "receipt")}
    values = pcm(run / "state/native-audio.wav")
    for name, snap in snapshots.items():
        if "audio_preview" not in snap:
            continue
        stats = snap["audio_preview"]
        assert stats["enabled"] and stats["errors"] == 0 and stats["dropped"] == 0, (name, stats)
        assert stats["capture_dropped"] == 0, (name, "incomplete capture")
        assert snap["document"] == snapshots["editing"]["document"], (name, "document changed")
        assert not snap["dirty"] and not snap["compile"] and not snap["play"], name
        if name.startswith("stopped") or name in ("editing", "focus_stopped", "final", "audition_finished"):
            assert not voices(snap), (name, voices(snap))
    measurements = {}
    if case == "lifecycle":
        for name in ("near", "lights_on", "restarted"):
            snap = snapshots[name]
            for cue in ("ambience_hvac", "fluorescent_buzz", "pool_lapping", "empty_room", "water_emitter"):
                voice = one(snap, cue)
                assert voice["spatial"] and voice["looping"] and voice["gain"] > 0, (name, voice)
            assert len(voices(snap)) == 5, (name, voices(snap))
        assert not voices(snapshots["lights_off"], "fluorescent_buzz"), "switched-off hum continues"
        assert len(voices(snapshots["lights_off"])) == 4, "light switching must preserve other loops"
        one(snapshots["door_opening"], "door_open")
        one(snapshots["door_closing"], "door_close")
        audition = one(snapshots["audition"], "water_emitter")
        assert not audition["spatial"] and not audition["looping"], audition
        measurements["near_rms"] = rms_before(values, snapshots["near"])
        assert sum(measurements["near_rms"]) > 1e-5, measurements
        measurements["stopped_rms"] = rms_before(values, snapshots["stopped"])
        assert max(measurements["stopped_rms"]) < 1e-7, measurements
    elif case == "hvac_move":
        one(snapshots["near"], "ambience_hvac")
        one(snapshots["returned"], "ambience_hvac")
        assert not voices(snapshots["walk_cutoff"]), "leaving duct range must fade and retire the live loop"
        measurements["walk_cutoff_rms"] = rms_before(values, snapshots["walk_cutoff"])
        assert max(measurements["walk_cutoff_rms"]) < 1e-7, measurements
        measurements["returned_rms"] = rms_before(values, snapshots["returned"])
        assert sum(measurements["returned_rms"]) > 1e-5, measurements
    elif case.startswith("hvac"):
        for name in ("near", "left", "right", "mid"):
            v = one(snapshots[name], "ambience_hvac")
            assert len(voices(snapshots[name])) == 1 and v["spatial"], (name, v)
            measurements[name] = {"voice": v, "rms": rms_before(values, snapshots[name])}
        assert not voices(snapshots["cutoff"]), "HVAC cutoff must retire its source"
        measurements["cutoff_rms"] = rms_before(values, snapshots["cutoff"])
        assert max(measurements["cutoff_rms"]) < 1e-7, measurements
        assert measurements["near"]["voice"]["gain"] > measurements["mid"]["voice"]["gain"] > 0, measurements
        assert sum(measurements["near"]["rms"]) > sum(measurements["mid"]["rms"]) * 2, measurements
        assert measurements["left"]["voice"]["pan"] > 0.1 and measurements["right"]["voice"]["pan"] < -0.1, measurements
        left, right = measurements["left"]["rms"], measurements["right"]["rms"]
        assert left[1] > left[0] * 1.2 and right[0] > right[1] * 1.2, measurements
    elif case == "steps":
        walking = [snap for name, snap in snapshots.items() if name.startswith("walk_")]
        assert any(voices(snap, "footstep_wood") for snap in walking), "missing wood footsteps"
        assert any(voices(snap, "footstep_carpet") for snap in walking), "missing carpet footsteps"
        assert snapshots["rest"]["audio_preview"]["played"] > snapshots["idle"]["audio_preview"]["played"], "walking must start actual mixer voices"
        assert not voices(snapshots["idle"]) and not voices(snapshots["rest"]), "idle footsteps"
    else:
        assert not voices(snapshots["empty"]), "empty scene inherited environment audio"
    return measurements


def run_case(args, case):
    run = args.out / case
    run.mkdir(parents=True, exist_ok=False)
    family = "hvac" if case.startswith("hvac") else {"lifecycle": "all"}.get(case, case)
    level = fixture(family, 12 if case == "hvac_duplicate" else 1)
    project = run / "project"
    write_json(project / "levels/quickplay_audio.json", level)
    write_json(project / "places.project.json", {"schema": "places-project", "project_format": 1,
        "id": "quickplay_audio", "name": "Quick-play audio", "entry_level": "quickplay_audio",
        "levels": [{"id": "quickplay_audio", "source": "levels/quickplay_audio.json"}]})
    cues = [v["id"] for v in json.loads((args.assets / "audio/catalog.json").read_text())["cues"]
            if v["id"] != "ambience_hvac"]
    write_json(run / "script.json", {"output": str(run), "actions": actions(case, cues.index("water_emitter"))})
    env = dict(os.environ, PLACES_STATE_ROOT=str(run / "state"),
        PLACES_CONFIG_ROOT=str(run / "config"), PLACES_CACHE_ROOT=str(run / "cache"),
        PLACES_ASSET_ROOT=str(args.assets), PLACES_BENCH="1", PLACES_AUDIO_CAPTURE="1", PLACES_VERBOSE="1")
    env.pop("PLACES_AUDIO_DISABLE", None)
    command = [str(args.binary), str(project), "--asset-root", str(args.assets),
               "--verify-script", str(run / "script.json"), "--capture-dir", str(run)]
    with (run / "native.log").open("w") as log:
        result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
    assert result.returncode == 0, (case, result.returncode, run / "native.log")
    measurements = validate(case, run)
    write_json(run / "receipt.json", {"passed": True, "case": case, "command": command,
        "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
        "measurements": measurements, "bakes": 0,
        "limits": "Mac native SDL mixer/Metal; no physical speaker listening or other-platform claim"})
    print(case, "passed", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--assets", type=Path, default=ROOT / "assets")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--case", choices=["lifecycle", "hvac", "hvac_duplicate", "hvac_move", "steps", "empty"])
    args = parser.parse_args()
    args.binary = args.binary.resolve(strict=True)
    args.assets = args.assets.resolve(strict=True)
    args.out = args.out.resolve()
    cases = [args.case] if args.case else ["lifecycle", "hvac", "hvac_duplicate", "hvac_move", "steps", "empty"]
    for case in cases:
        run_case(args, case)
    if not args.case:
        single = json.loads((args.out / "hvac/receipt.json").read_text())["measurements"]["near"]
        duplicate = json.loads((args.out / "hvac_duplicate/receipt.json").read_text())["measurements"]["near"]
        assert abs(single["voice"]["gain"] - duplicate["voice"]["gain"]) < 0.001
        ratio = sum(duplicate["rms"]) / sum(single["rms"])
        assert 0.85 < ratio < 1.15, ("duplicate ducts changed output", ratio)
        write_json(args.out / "passed.json", {"cases": cases, "duplicate_rms_ratio": ratio, "bakes": 0})


if __name__ == "__main__":
    main()

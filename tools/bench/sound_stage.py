#!/usr/bin/env python3
"""Explicit native audio acceptance; never builds, bakes or changes permissions.

Compile tests/fixtures/native/sound.level.json separately, then run each mode:
  python3 tools/bench/sound_stage.py --binary /absolute/places --map /absolute/sound.placesmap --mode controls --out local/sound-controls
  python3 tools/bench/sound_stage.py --binary /absolute/places --map /absolute/sound.placesmap --mode movement --capture-audio
  python3 tools/bench/sound_stage.py --binary /absolute/places --map /absolute/sound.placesmap --mode lifecycle
  python3 tools/bench/sound_stage.py --binary /absolute/places --map /absolute/sound.placesmap --mode lifecycle --unavailable-device
  python3 tools/bench/sound_stage.py --binary /absolute/places --map /absolute/sound.placesmap --mode settings

Only a direct executable is launched, never an .app wrapper. --plan-only writes
plans without launching it; --map is optional in that mode. Runs own their state
and copied package. Native action_result snapshots provide bounded observations
between ordinary input edges; F12 releases are inert observation markers.
Player/item poses establish explicit fixtures, never stand in for walking.
--capture-audio (or PLACES_AUDIO_CAPTURE=1) retains genuine post-mixer float WAV
evidence. Statistics and PCM measurements do not prove the sound was heard or
that it sounds right: listen to native-audio.wav and review the retained PNGs.
--no-device (or PLACES_AUDIO_DISABLE=1) explicitly disables audio. The mutually
exclusive --unavailable-device instead injects a subsystem initialization
failure through an invalid process-local SDL_AUDIODRIVER, leaving
PLACES_AUDIO_DISABLE unset. Both check silent fallback; receipts distinguish
the two causes. Neither changes operating-system settings.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time

from console_stage import detail, digest, evidence, write_json
from item_physics import Plan

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = ROOT / "tests/fixtures/native/sound.level.json"
LEVEL = "sound_native"
WATER = "water_emitter"
UNAVAILABLE_AUDIO_DRIVER = "places_acceptance_unavailable_driver"


class Stage:
    """Small sequential scripts; markers observe real native action results."""

    def __init__(self) -> None:
        self.plan = Plan(LEVEL)
        self.ms = 400
        self.markers: dict[str, int] = {}

    def wait(self, ms: int = 160) -> None:
        self.ms += ms

    def action(self, kind: str, **fields: object) -> None:
        self.plan.action(self.ms, kind, **fields)
        self.ms += 100

    def tap(self, key: str, gap: int = 130) -> None:
        self.plan.tap(self.ms, key)
        self.ms += gap

    def hold(self, key: str, ms: int = 650) -> None:
        self.plan.key(self.ms, key, True)
        self.plan.key(self.ms + ms, key, False)
        self.ms += ms + 100

    def sample(self, name: str, wait: int = 120) -> None:
        self.wait(wait)
        self.markers[name] = len(self.plan.steps)
        self.action("key", key="F12", pressed=False)

    def capture(self, name: str) -> None:
        self.action("capture", name=name)
        self.wait(1100)

    def player(self, x: float, z: float, yaw: float = 0,
               pitch: float = 0) -> None:
        self.action("player", position=[x, 1.6, z], yaw=yaw, pitch=pitch)

    def item(self, name: str, x: float, z: float) -> None:
        self.action("item_pose", id=name, position=[x, .8, z], rotation=[0, 0, 0, 1])

    def command(self, text: str) -> None:
        self.action("text", text=text, submit=True)

    def console(self) -> None:
        self.action("console_enabled", enabled=True)
        self.tap("`")

    def finish(self) -> tuple[list[dict], dict[str, int]]:
        return self.plan.finish(self.ms + 250), self.markers


def make_plan(mode: str) -> tuple[list[dict], dict[str, int]]:
    s = Stage()
    s.action("focus", focused=True)
    s.wait(250)
    s.sample("baseline")
    if mode == "controls":
        # Real E presses exercise switch feedback and the authored cooldown.
        s.player(-1.8, -1.8, 270, -15)
        s.sample("switch-before")
        s.tap("E")
        s.tap("E")
        s.sample("switch-rapid")
        s.wait(650)
        s.tap("E")
        s.sample("switch-repeat")
        s.player(2, 0, 90, -15)
        s.sample("door-before")
        for name, yaw in (("door-open", 90), ("door-close", 113.2), ("door-reopen", 90)):
            s.player(2.4, 0, yaw, -12)
            s.tap("E")
            s.sample(name, 650)

        # An unheld exact instance must not satisfy the held filter.
        s.item("native_duck", 0, .6)
        s.sample("exact-unheld")
        s.player(0, 1.65, 0, -30)
        s.action("aim_item", id="native_duck")
        s.tap("E")
        s.sample("exact-held", 30)
        s.hold("S", 100)
        s.sample("source-moved", 20)
        s.tap("E")
        s.item("native_duck", -1.4, -1.5)
        s.item("other_duck", 0, .6)
        s.wait(650)
        s.player(0, 1.65, 0, -30)
        s.action("aim_item", id="other_duck")
        s.sample("wrong-before")
        s.tap("E")
        s.sample("wrong-held")
        s.player(6, -1.4, 0, -30)
        s.sample("type-held")
        s.player(0, 1.65, 0, -30)
        s.tap("E")
        s.item("other_duck", -.5, -1.5)
        s.sample("type-before")
        s.item("native_duck", 5.8, -2.2)
        s.sample("type-first")
        s.wait(650)
        s.item("other_duck", 7, -2.2)
        s.sample("type-second")
        # Re-enter inside the shared binding's cooldown, then after it expires.
        s.item("other_duck", 7, -.9)
        s.item("other_duck", 7, -2.2)
        s.sample("type-cooldown", 30)
        s.wait(650)
        s.item("other_duck", 7, -.9)
        s.item("other_duck", 7, -2.2)
        s.sample("type-repeat")
        # The door is open. Ordinary motion sweeps the 3 cm once-only volume.
        s.player(3.25, 0, 90)
        s.sample("once-before")
        s.hold("W", 230)
        s.sample("once-first")
        s.hold("S", 230)
        s.hold("W", 230)
        s.sample("once-repeat")
        s.capture("controls-final")
    elif mode == "movement":
        for name, x in (("wood", -1.8), ("carpet", 4), ("hard", 5.7), ("gravel", 7.6)):
            s.player(x, 1.9)
            s.sample(f"walk-{name}-before")
            s.hold("W")
            s.sample(f"walk-{name}-after")
        s.console()
        s.command("/time scale 0")
        s.action("escape")
        s.player(-1.8, 1.9)
        s.sample("zero-before")
        s.hold("W")
        s.sample("zero-after")
        s.action("escape")
        s.sample("pause-before")
        s.hold("W", 500)
        for _ in range(3):
            s.tap("E", 100)
        s.sample("pause-after")
        s.action("escape")
        s.sample("resumed")
        s.capture("movement-final")
    elif mode == "lifecycle":
        s.action("activate", id="fixture_activate")
        s.player(-1.8, .8)
        s.sample("near")
        s.player(-1.8, .8, 180)
        s.sample("turned")
        s.player(2, .8)
        s.sample("far")
        # Pose outside the region but inside the room, then cross its boundary
        # with ordinary input: a teleport re-seeds occupancy without entering.
        s.player(3.5, 1.5, 90)
        s.sample("region-outside")
        s.hold("W")
        s.sample("both-running")
        s.console()
        s.command("/time scale 0")
        s.action("escape")
        s.sample("world-zero")
        s.console()
        s.command("/time scale 1")
        s.action("escape")
        s.sample("world-resumed")
        s.action("escape")
        s.sample("paused")
        s.action("escape")
        s.sample("unpaused")
        s.action("focus", focused=False)
        s.sample("unfocused")
        s.action("focus", focused=True)
        s.sample("refocused")
        # Change the visual camera through normal console input, keeping the
        # player still. The spatial listener must stay at the player's ears.
        s.console()
        s.command("/become party_cup")
        s.action("escape")
        s.sample("representation-cup")
        s.console()
        s.command("/become reset")
        s.action("escape")
        s.sample("representation-reset")
        s.action("activate", id="fixture_reset")
        s.sample("stopped", 300)
        s.action("activate", id="fixture_activate")
        s.sample("before-reload")
        s.action("load", level=LEVEL)
        s.sample("reloaded", 3500)
        s.capture("lifecycle-final")
    else:
        s.action("activate", id="fixture_activate")
        # Pause -> Settings -> Audio, all through the ordinary SDL key path.
        s.action("escape")
        s.tap("Down")
        s.tap("Return")
        s.tap("Down")
        s.tap("Down")
        s.tap("Return")
        s.sample("audio-page")
        for _ in range(3):
            s.tap("Left")
            s.tap("Down")
        s.tap("Return")
        s.sample("muted")
        s.capture("audio-muted")
        s.tap("Return")
        s.sample("unmuted")
        s.tap("Return")
        s.sample("muted-again")
        for _ in range(3):
            s.action("escape")
        s.sample("muted-playing")
        s.action("load", level=LEVEL)
        s.sample("settings-reloaded", 3500)
        s.capture("settings-final")
    return s.finish()


def validate(receipt: dict, events: list[dict], steps: list[dict], markers: dict[str, int],
             out: Path, log: str, expect_silent: bool) -> None:
    checks = receipt["checks"]

    def require(name: str, passed: bool) -> None:
        checks.append(dict(name=name, passed=bool(passed)))

    captures, results, seen = evidence(events)
    samples = {name: results.get(index, {}) for name, index in markers.items()}
    receipt.update(snapshots=samples, captures=captures)
    action_indices = {detail(event).get("id"): index for index, event in enumerate(events)
                      if event.get("event") == "action"}

    def between(first: str, last: str, kind: str, cue: str | None = None) -> list[dict]:
        start, end = action_indices.get(markers[first]), action_indices.get(markers[last])
        if start is None or end is None:
            return []
        return [detail(event) for event in events[start:end]
                if event.get("event") == kind and (cue is None or detail(event).get("cue") == cue)]

    def voice(name: str, cue: str) -> dict:
        matches = [v for v in samples.get(name, {}).get("audio_voices", []) if v.get("cue") == cue]
        # Pickup feedback and an authored moving source may share a cue.
        return next((v for v in matches if v.get("source") is not None), matches[0] if matches else {})

    def audio(name: str) -> dict:
        return samples.get(name, {}).get("audio", {})

    require("native exit zero", receipt.get("exit_code") == 0)
    require("all bounded native actions execute", seen == set(range(len(steps))))
    require("every observation has an actual native snapshot", all(samples.values()))
    require("fixture committed natively", any(event.get("event") == "world_committed"
            and detail(event).get("current_level_id") == LEVEL for event in events))
    require("native frames presented", any(event.get("event") == "present" and event.get("detail") == "ready" for event in events))
    require("no renderer or process failure", not any(token in log.lower() for token in
            ("validation error", "panicked", "fatal device", "catalog hash mismatch")))
    require("no physics escape recovery", not any(event.get("event") == "physics_recovery" for event in events))
    require("native window actually has input focus", samples["baseline"].get("window_native_focus") is True)
    require("audio device state matches requested acceptance", all(audio(name).get("enabled") is (not expect_silent) for name in samples))
    require("voice counts remain bounded", all(audio(name).get("active_voices", 999) <= 48
            and audio(name).get("pending_voices", 999) <= 48 for name in samples))
    if not expect_silent:
        require("native callback really mixes frames", max((audio(name).get("output_frames", 0) for name in samples), default=0) > 0)
        require("clips decoded once within the audio budget", all(0 < audio(name).get("decoded_bytes", 0) <= 64 * 1024 * 1024 for name in samples))
        require("device reports no callback or loading errors", all(audio(name).get("errors") == 0 for name in samples))
        require("accepted native playback exists", max((audio(name).get("played", 0) for name in samples), default=0) > 0)
        requests = [detail(event) for event in events if event.get("event") in ("audio_cue", "audio_authored", "audio_door")]
        require("every traced cue request reaches native playback", bool(requests) and all(row.get("played") is True for row in requests))
    else:
        require("silent fallback owns no voices or device frames", all(all(audio(name).get(key) == 0
                for key in ("active_voices", "pending_voices", "played", "output_frames")) for name in samples))
        if receipt["unavailable_device"]:
            require("injected initialization failure reaches silent fallback", "[audio] silent fallback:" in log
                    and all(audio(name).get("errors") == 1 for name in samples))
    for step in steps:
        action = step["action"]
        if action["kind"] == "capture":
            path = out / f"{action['name']}.png"
            header = path.read_bytes()[:24] if path.is_file() else b""
            valid = len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n"
            require(f"{action['name']}: real native PNG and snapshot", valid and action["name"] in captures)
            if valid:
                receipt["screenshots"].append(dict(path=str(path), sha256=digest(path),
                    size=[int.from_bytes(header[16:20], "big"), int.from_bytes(header[20:24], "big")]))

    mode = receipt["mode"]
    if mode == "controls":
        for name, first, last, count in (("rapid switch cooldown", "switch-before", "switch-rapid", 1),
                ("switch rearms", "switch-rapid", "switch-repeat", 1),
                ("unheld exact duck rejected", "door-reopen", "exact-unheld", 0),
                ("held exact duck accepted", "exact-unheld", "exact-held", 1),
                ("other held instance rejected", "wrong-before", "wrong-held", 0)):
            cue = "prop_bell" if "switch" in name else "prop_duck"
            require(name, len(between(first, last, "audio_authored", cue)) == count)
        require("light switch emits feedback", len(between("switch-before", "switch-repeat", "audio_cue", "switch_click")) == 2)
        doors = between("door-before", "door-reopen", "audio_door")
        require("door phase changes sound repeatedly", [row.get("cue") for row in doors] == ["door_open", "door_close", "door_open"])
        require("real duck pickup emits prop_duck", bool(between("exact-unheld", "exact-held", "audio_cue", "prop_duck")))
        held = samples["exact-held"].get("physics", {}).get("held")
        require("ordinary E actually holds the exact duck", held is not None and held == samples["source-moved"].get("physics", {}).get("held"))
        authored = samples["exact-held"].get("physics", {}).get("authored", [])
        native_key = next((row[0] for row in authored if row[1] == "native_duck"), None)
        require("held body is the requested authored instance", native_key is not None and held == native_key)
        require("type filter rejects a held duck", samples["type-held"].get("physics", {}).get("held") is not None
                and not between("wrong-held", "type-held", "audio_authored", "prop_bell"))
        if not expect_silent:
            first, moved = voice("exact-held", "prop_duck"), voice("source-moved", "prop_duck")
            require("authored held source follows ordinary movement", first.get("source") is not None
                    and first.get("source") == moved.get("source") and first.get("position") is not None
                    and moved.get("position") is not None and math.dist(first["position"], moved["position"]) > .05)
        for name, first, last, count in (("type accepts native duck", "type-before", "type-first", 1),
                ("type accepts another duck", "type-first", "type-second", 1),
                ("type binding respects cooldown", "type-second", "type-cooldown", 0),
                ("type binding rearms", "type-cooldown", "type-repeat", 1),
                ("thin swept volume fires once", "once-before", "once-first", 1),
                ("once binding does not repeat", "once-first", "once-repeat", 0)):
            require(name, len(between(first, last, "audio_authored", "prop_bell")) == count)
    elif mode == "movement":
        for surface in ("wood", "carpet", "hard", "gravel"):
            before, after = f"walk-{surface}-before", f"walk-{surface}-after"
            require(f"{surface}: ordinary W moves grounded player", math.dist(samples[before].get("player_position", [0, 0, 0]),
                    samples[after].get("player_position", [0, 0, 0])) > 1 and samples[after].get("player_body", {}).get("grounded") is True)
            require(f"{surface}: matching material footsteps", bool(between(before, after, "audio_cue", f"footstep_{surface}")))
        require("zero world time is native clock state", samples["zero-after"].get("physics_clock", {}).get("time_scale") == 0)
        require("walking at zero scale advances no world ticks", samples["zero-before"].get("physics_clock", {}).get("fixed_steps")
                == samples["zero-after"].get("physics_clock", {}).get("fixed_steps"))
        require("real movement footsteps continue at zero world time", bool(between("zero-before", "zero-after", "audio_cue", "footstep_wood")))
        require("paused menu cannot generate footsteps", not any(row.get("cue", "").startswith("footstep_")
                for row in between("pause-before", "pause-after", "audio_cue")))
        require("rapid menu input preserves pause then resumes", samples["pause-after"].get("state") == "Paused" and samples["resumed"].get("state") == "Playing")
    elif mode == "lifecycle":
        outside = samples["region-outside"].get("player_position")
        inside = samples["both-running"].get("player_position")
        require("ordinary W enters the aggregate audio region", outside is not None and inside is not None
                and outside[0] < 4 <= inside[0] <= 8 and -.5 <= inside[2] <= 2.5
                and math.dist(outside, inside) > 1)
        require("aggregate entry requests its real-time loop", len(between("region-outside", "both-running", "audio_authored", "ambience_hvac")) == 1)
        cup = samples["representation-cup"].get("representation", {})
        require("console selects the party cup in third person", cup.get("third_person") is True
                and cup.get("model") == "arcade:party_cup_confetti")
        require("console reset restores first person", samples["representation-reset"].get("representation", {}).get("third_person") is False)
        listener = samples["refocused"]
        for name in ("representation-cup", "representation-reset"):
            state = samples[name]
            position, original = state.get("player_position"), listener.get("player_position")
            require(f"{name}: player ears retain the same pose and look", position is not None and original is not None
                    and math.dist(position, original) < .0001
                    and all(math.isclose(state.get(key, -100), listener.get(key, 100), abs_tol=.000001)
                            for key in ("player_yaw", "player_pitch"))
                    and state.get("console_open") is False and state.get("state") == "Playing")
        if not expect_silent:
            near, turned, far = (voice(name, WATER) for name in ("near", "turned", "far"))
            require("loop has stable authored source", near.get("source") is not None and near.get("source") == far.get("source") and near.get("looping") is True)
            require("listener distance attenuates spatial loop", near.get("gain", 0) > far.get("gain", 1) > 0)
            require("listener turn reverses spatial pan", near.get("pan", 0) * turned.get("pan", 0) < -.1)
            for name, world_paused, real_paused in (("world-zero", True, False), ("world-resumed", False, False),
                    ("paused", True, True), ("unpaused", False, False),
                    ("unfocused", True, True), ("refocused", False, False)):
                world, real = voice(name, WATER), voice(name, "ambience_hvac")
                require(f"{name}: world and real clocks follow policy", bool(world) and bool(real)
                        and world.get("paused") is world_paused and real.get("paused") is real_paused
                        and real.get("playback_rate") == 1)
            require("zero world scale reaches playback rate", voice("world-zero", WATER).get("playback_rate") == 0)
            for name in ("representation-cup", "representation-reset"):
                for cue in (WATER, "ambience_hvac"):
                    original_voice, current_voice = voice("refocused", cue), voice(name, cue)
                    require(f"{name}: {cue} mix stays at the player's ears", original_voice.get("source") is not None
                            and current_voice.get("source") == original_voice.get("source")
                            and all(math.isclose(current_voice.get(key, -100), original_voice.get(key, 100),
                                                 rel_tol=.00001, abs_tol=.00001) for key in ("gain", "pan")))
            require("explicit stop retires both source voices", not samples["stopped"].get("audio_voices"))
            require("loop really restarts before reload", bool(voice("before-reload", WATER)))
        require("reload commits and clears old source voices", sum(event.get("event") == "world_committed" for event in events) >= 2
                and samples["reloaded"].get("current_level_id") == LEVEL and not samples["reloaded"].get("audio_voices"))
        require("reload retains decoded library", audio("baseline").get("decoded_bytes") == audio("reloaded").get("decoded_bytes"))
    else:
        for name in ("muted", "muted-again", "muted-playing", "settings-reloaded"):
            settings = samples[name].get("audio_settings", {})
            require(f"{name}: menu preserves volume choices and mute", settings.get("muted") is True
                    and all(math.isclose(settings.get(key, -1), .95, abs_tol=.0001) for key in ("master", "effects", "ambience")))
        require("real menu can unmute", samples["unmuted"].get("audio_settings", {}).get("muted") is False)
        require("menu returns to actual play", samples["muted-playing"].get("state") == "Playing")
        if not expect_silent:
            require("mute reaches the live mixer", bool(voice("muted-playing", WATER)) and voice("muted-playing", WATER).get("gain") == 0)
        persisted = json.loads((out / "settings.json").read_text())
        receipt["saved_audio_settings"] = {key: persisted.get(key) for key in ("master_volume", "effects_volume", "ambient_volume", "muted")}
        require("real menu saves audio preferences", persisted.get("muted") is True and all(
                math.isclose(persisted.get(key, -1), .95, abs_tol=.0001) for key in ("master_volume", "effects_volume", "ambient_volume")))
    shutdown = [detail(event) for event in events if event.get("event") == "audio_shutdown"]
    require("shutdown clears live and queued voices", bool(shutdown) and shutdown[-1].get("active_voices") == 0 and shutdown[-1].get("pending_voices") == 0)


def pcm_evidence(path: Path) -> dict:
    data = path.read_bytes()
    if len(data) < 44 or data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("native audio capture is not a complete RIFF WAV")
    fmt, channels, rate = struct.unpack_from("<HHI", data, 20)
    size = struct.unpack_from("<I", data, 40)[0]
    if fmt != 3 or channels != 2 or rate != 48_000 or size != len(data) - 44 or size % 8:
        raise ValueError("native audio capture does not match stereo float32 48 kHz contract")
    count, peak, energy = 0, 0.0, 0.0
    for (value,) in struct.iter_unpack("<f", data[44:]):
        if not math.isfinite(value):
            raise ValueError("native mixer emitted non-finite PCM")
        count += 1
        peak = max(peak, abs(value))
        energy += value * value
    return dict(path=str(path), sha256=digest(path), frames=count // 2,
                sample_rate=rate, peak=peak, rms=math.sqrt(energy / count) if count else 0,
                listening_review="required; PCM measurements do not establish perceived sound quality")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, required=True, help="Direct native executable, never an .app wrapper")
    parser.add_argument("--map", type=Path, help="Explicit separately compiled sound_native .placesmap")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--mode", choices=("controls", "movement", "lifecycle", "settings"), required=True)
    parser.add_argument("--plan-only", action="store_true")
    parser.add_argument("--capture-audio", action="store_true", default=os.environ.get("PLACES_AUDIO_CAPTURE") == "1")
    audio_mode = parser.add_mutually_exclusive_group()
    audio_mode.add_argument("--no-device", action="store_true", help="Force audio disabled with PLACES_AUDIO_DISABLE=1")
    audio_mode.add_argument("--unavailable-device", action="store_true",
                           help="Inject audio initialization failure with an invalid process-local SDL audio driver")
    args = parser.parse_args()
    args.no_device = args.no_device or (os.environ.get("PLACES_AUDIO_DISABLE") == "1" and not args.unavailable_device)
    expect_silent = args.no_device or args.unavailable_device
    if not args.plan_only and args.map is None:
        parser.error("--map is required for native execution; compile the fixture separately")
    binary = args.binary.resolve(strict=not args.plan_only)
    if any(part.endswith(".app") for part in binary.parts):
        parser.error("--binary must be a direct executable outside an .app wrapper")
    if not args.plan_only and (not binary.is_file() or not os.access(binary, os.X_OK)):
        parser.error("--binary must already be an executable file; this tool does not change permissions")
    out = (args.out or ROOT / "local/sound-0.11.0" / f"{datetime.now(timezone.utc):%Y%m%dT%H%M%SZ}-{args.mode}").resolve()
    out.mkdir(parents=True, exist_ok=False)
    steps, markers = make_plan(args.mode)
    settings = json.loads((ROOT / "tests/fixtures/native/retro-arcade-manifest.json").read_text())["settings"]
    settings.update(window_mode="fullscreen", window_width=1280, window_height=720, vsync=False,
                    developer_console=False, master_volume=1, effects_volume=1, ambient_volume=1, muted=False)
    write_json(out / "settings.json", settings)
    write_json(out / "actions.json", steps)
    receipt = dict(mode=args.mode, status="planned", binary=str(binary), fixture_sha256=digest(FIXTURE),
                   actions=len(steps), duration_ms=steps[-1]["delay_ms"], markers=markers, checks=[], screenshots=[],
                   requested_vsync=False, requested_window_mode="fullscreen", no_device=args.no_device,
                   unavailable_device=args.unavailable_device,
                   audio_backend_mode="initialization_failure" if args.unavailable_device else "forced_disabled" if args.no_device else "native",
                   listening_review="required; native trace counters and voice snapshots do not prove sound was heard",
                   visual_review="required; inspect retained native screenshots", performance_claim="none")
    if binary.is_file():
        receipt["binary_sha256"] = digest(binary)
    package = None
    if args.map is not None:
        package = args.map.resolve(strict=True)
        if not package.is_file() or package.suffix != ".placesmap":
            raise ValueError("--map must name an existing separately compiled .placesmap")
        (out / "levels").mkdir()
        shutil.copyfile(package, out / "levels" / f"{LEVEL}.placesmap")
        receipt.update(package=str(package), package_sha256=digest(package))
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_") and key != "SDL_VIDEODRIVER"}
    env.update(PLACES_ASSET_ROOT=str(ROOT), PLACES_STATE_ROOT=str(out), PLACES_LEVEL=LEVEL,
               PLACES_BENCH="1", PLACES_VERBOSE="1", PLACES_VSYNC="0", PLACES_BENCH_FRAMES="0",
               PLACES_BENCH_WARMUP="0", PLACES_BENCH_ACTIONS=str(out / "actions.json"),
               PLACES_LOAD_TRACE=str(out / "trace.jsonl"), PLACES_STATE_LOG=str(out / "player.csv"),
               PLACES_BENCH_OUT=str(out / "frames.csv"))
    if args.capture_audio:
        env["PLACES_AUDIO_CAPTURE"] = "1"
    if args.no_device:
        env["PLACES_AUDIO_DISABLE"] = "1"
    if args.unavailable_device:
        env["SDL_AUDIODRIVER"] = UNAVAILABLE_AUDIO_DRIVER
    receipt["environment"] = {key: value for key, value in env.items() if key.startswith("PLACES_") or key == "SDL_AUDIODRIVER"}
    write_json(out / "receipt.json", receipt)
    if args.plan_only:
        print(json.dumps(dict(status="planned", out=str(out), actions=len(steps), duration_ms=receipt["duration_ms"])))
        return 0
    started = time.monotonic()
    try:
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, capture_output=True, text=True, timeout=90)
        log = result.stdout + result.stderr
        (out / "native.log").write_text(log)
        receipt["exit_code"] = result.returncode
        events = [json.loads(line) for line in (out / "trace.jsonl").read_text().splitlines()]
        receipt["native_identity"] = [line for line in log.splitlines() if line.startswith(("[renderer]", "[window]", "[settings]", "[audio]"))]
        validate(receipt, events, steps, markers, out, log, expect_silent)
        if args.capture_audio:
            receipt["pcm"] = pcm_evidence(out / "native-audio.wav")
            if not expect_silent:
                receipt["checks"].append(dict(name="genuine mixer PCM has finite nonzero samples", passed=receipt["pcm"]["frames"] > 0 and receipt["pcm"]["peak"] > 0))
        receipt["checks"].append(dict(name="binary and source package remain unchanged",
                passed=digest(binary) == receipt["binary_sha256"] and package is not None and digest(package) == receipt["package_sha256"]))
        failures = [check["name"] for check in receipt["checks"] if not check["passed"]]
        if failures:
            raise ValueError("Native assertions failed: " + "; ".join(failures))
        receipt["status"] = "passed"
    except subprocess.TimeoutExpired as error:
        def decoded(value: str | bytes | None) -> str:
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        (out / "native.log").write_text(decoded(error.stdout) + decoded(error.stderr))
        receipt.update(status="failed", error="Native executable exceeded 90 seconds")
    except Exception as error:
        receipt.update(status="failed", error=str(error))
    finally:
        receipt["elapsed_seconds"] = time.monotonic() - started
        write_json(out / "receipt.json", receipt)
    print(json.dumps(dict(status=receipt["status"], out=str(out), checks=len(receipt["checks"]), error=receipt.get("error"))))
    return 0 if receipt["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())

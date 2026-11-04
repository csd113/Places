#!/usr/bin/env python3
"""Run the native elevator acceptance campaign with direct executables.

No build or bake occurs here. Supply a current Static export of
projects/elevator-lab. Every case uses a fresh isolated user-data directory.
The controlled failure renames only that case's disposable installed package.
These bounded runs exercise real input, travel, frozen loading, saves,
rollback and spatial audio; this tool is never automatic CI or a Rust test.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
LOBBY_READY = "ready:elevator_lobby"
ANNEX_REQUEST = "request:elevator_annex"
ANNEX_READY = "ready:elevator_annex"
ANNEX_FAILED = "failed:elevator_annex"


def actions(mode):
    steps = []

    def add(delay, kind, after=LOBBY_READY, **details):
        steps.append({
            "after": after,
            "delay_ms": delay,
            "action": {"kind": kind, **details},
        })

    def click(delay, after=LOBBY_READY):
        add(delay, "mouse", after, button="left", pressed=True)
        add(delay + 70, "mouse", after, button="left", pressed=False)

    if mode == "links":
        anchor = "ready:elevator_groups"
        def put(ms, kind, **kw): add(ms, kind, anchor, **kw)
        def tap(ms): click(ms, anchor)
        put(100,"player",position=[0,1.6,1.2],yaw=0,pitch=0)
        put(180,"view_at",position=[.94,1.58,.4]);put(250,"capture",name="three-stop-two-buttons");tap(400)
        put(4500,"checkpoint",name="links_same_height")
        put(4600,"view_at",position=[10.94,1.47,.4]);tap(4800)
        put(10300,"checkpoint",name="links_upper")
        put(10500,"save_checkpoint",name="linked_upper");put(10900,"resume_checkpoint")
        put(13200,"checkpoint",name="links_resumed")
        put(13400,"view_at",position=[10.94,8.58,.4]);tap(13600)
        put(19300,"checkpoint",name="links_ground")
        put(19500,"player",position=[20,1.6,1.2],yaw=0,pitch=0)
        put(19600,"view_at",position=[20.94,1.58,.4]);put(19800,"capture",name="two-stop-one-button");tap(20000)
        put(24400,"checkpoint",name="links_other_upper")
        put(24600,"view_at",position=[20.94,5.08,.4]);tap(24800)
        put(30400,"checkpoint",name="links_other_ground");put(30700,"capture",name="independent-group-return");put(31000,"quit")
        return steps

    if mode == "views":
        add(100, "player", position=[0, 1.6, -2.65], yaw=180, pitch=0)
        add(200, "view_at", position=[0, 1.15, 0])
        add(350, "capture", name="elevator-closed")
        add(550, "player", position=[-0.96, 1.6, -1], yaw=180, pitch=0)
        add(650, "view_at", position=[-0.96, 1.08, -0.12])
        click(750)
        add(2350, "player", position=[0, 1.6, -2.65], yaw=180, pitch=0)
        add(2450, "view_at", position=[0, 1.15, 1.0])
        add(2600, "capture", name="elevator-open")
        add(2850, "player", position=[0.75, 1.6, 0.45], yaw=180, pitch=0)
        add(3000, "view_at", position=[-0.3, 1.4, 2.2])
        add(3200, "capture", name="elevator-interior")
        add(3400, "player", position=[-0.6, 1.6, 1.90], yaw=0, pitch=0)
        add(3500, "view_at", position=[0.35, 1.25, 0.15])
        add(3700, "capture", name="elevator-controls")
        add(3900, "quit")
        return steps

    if mode == "seals":
        add(100, "player", position=[0, 1.6, .82], yaw=0, pitch=0)
        add(200, "capture", name="inside-closed-center")
        add(350, "player", position=[-.5, 2.05, .82], yaw=0, pitch=0)
        add(400, "view_at", position=[.66, 2.195, .1])
        add(500, "capture", name="inside-closed-top-right")
        add(650, "player", position=[.5, 1.6, .82], yaw=0, pitch=0)
        add(700, "view_at", position=[-.66, 1.3, .1])
        add(800, "capture", name="inside-closed-left")
        add(950, "player", position=[0, 1.6, 1.2], yaw=0, pitch=0)
        add(1000, "view_at", position=[.94, 1.58, .40])
        click(1100)
        add(1250, "view", yaw=0, pitch=0)
        add(1500, "capture", name="travel-start")
        add(1800, "capture", name="travel-closed")
        add(2200, "capture", name="travel-closed-upper")
        add(2600, "capture", name="travel-arrival")
        add(5200, "capture", name="doors-opening-early")
        add(5700, "capture", name="doors-opening-middle")
        add(6200, "capture", name="doors-opening-late")
        add(8300, "checkpoint", name="sealed-upper")
        add(8400, "player", position=[-.48, 5.1, -.65], yaw=180, pitch=0)
        add(8500, "view_at", position=[.25, 4.7, 1.5])
        add(8600, "capture", name="open-oblique-left")
        add(8750, "player", position=[.48, 5.1, -.65], yaw=180, pitch=0)
        add(8800, "view_at", position=[-.25, 4.7, 1.5])
        add(8900, "capture", name="open-oblique-right")
        add(9100, "quit")
        return steps

    if mode == "sound":
        add(100, "player", position=[0, 1.6, -0.8], yaw=180, pitch=0)
        add(8500, "checkpoint", name="near_sound")
        add(8600, "player", position=[8, 1.6, -0.8], yaw=180, pitch=0)
        add(9000, "checkpoint", name="far_sound")
        add(9100, "player", position=[0, 1.6, -0.8], yaw=180, pitch=0)
        add(9600, "checkpoint", name="near_return")
        add(9900, "quit")
        return steps

    add(100, "capture", name="closed")
    add(150, "player", position=[-0.96, 1.6, -1], yaw=180, pitch=0)
    add(200, "view_at", position=[-0.96, 1.08, -0.12])
    add(250, "capture", name="call_target")
    click(320)
    add(1850, "player", position=[0, 1.6, -2.65], yaw=180, pitch=0)
    add(1900, "view_at", position=[0, 1.15, 1.0])
    add(1950, "capture", name="open")
    add(2000, "player", position=[0, 1.6, -1], yaw=180, pitch=0)
    add(2050, "key", key="W", pressed=True)
    add(3050, "key", key="W", pressed=False)
    add(3250, "capture", name="inside")

    if mode == "flow":
        add(3350, "view_at", position=[0.94, 1.58, 0.40])
        add(3400, "capture", name="floor_target")
        click(3500)
        add(5200, "capture", name="moving")
        add(12000, "capture", name="upper")
        add(12030, "checkpoint", name="upper")
        add(12200, "save_checkpoint", name="upper_floor")
        add(12500, "resume_checkpoint")
        add(14800, "capture", name="upper_resumed")
        add(15000, "checkpoint", name="upper_resumed")
        add(15100, "view_at", position=[0.94, 4.97, 0.40])
        click(15300)
        add(40, "capture", ANNEX_REQUEST, name="frozen_a")
        add(220, "capture", ANNEX_REQUEST, name="frozen_b")
        add(350, "resize", ANNEX_REQUEST, width=1000, height=700)
        add(520, "capture", ANNEX_REQUEST, name="frozen_resized")
        add(700, "checkpoint", ANNEX_REQUEST, name="frozen_responsive")
        add(1800, "capture", ANNEX_READY, name="annex_arrival")
        add(1900, "view_at", ANNEX_READY, position=[0.94, 1.58, 0.40])
        click(2050, ANNEX_READY)
        add(7000, "capture", ANNEX_READY, name="return_lobby")
        add(7100, "checkpoint", ANNEX_READY, name="return_lobby")
        add(7500, "quit", ANNEX_READY)
        return steps

    add(3350, "view_at", position=[0.94, 1.47, 0.40])
    click(3500)
    if mode == "cancel":
        add(40, "capture", ANNEX_REQUEST, name="frozen_before_cancel")
        add(180, "escape", ANNEX_REQUEST)
        add(1900, "capture", ANNEX_REQUEST, name="cancel_reopened")
        add(2000, "checkpoint", ANNEX_REQUEST, name="cancel_recovered")
        add(2400, "quit", ANNEX_REQUEST)
    else:
        add(1800, "capture", ANNEX_FAILED, name="failure_reopened")
        add(1900, "checkpoint", ANNEX_FAILED, name="failure_recovered")
        add(2300, "quit", ANNEX_FAILED)
    return steps


def read_trace(path):
    rows = []
    if path.exists():
        for line in path.read_text().splitlines():
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                # The live reader can observe an incomplete final line.
                continue
    return rows


def assert_case(mode, state, rows):
    def snapshot(name):
        return next(json.loads(row["detail"]) for row in rows if row["event"] == name)

    if mode == "links":
        expected = {"links_same_height":(10,1.6),"links_upper":(10,8.6),"links_resumed":(10,8.6),"links_ground":(0,1.6),"links_other_upper":(20,5.1),"links_other_ground":(20,1.6)}
        for name,(x,y) in expected.items():
            value=snapshot("checkpoint:"+name)
            assert value["current_level_id"]=="elevator_groups" and not value["transition_frozen"],value
            assert abs(value["player_position"][0]-x)<.02 and abs(value["player_position"][1]-y)<.02,value
        first=snapshot("checkpoint:links_ground");last=snapshot("checkpoint:links_other_ground")
        assert first["elevators"][:3]==last["elevators"][:3],"unrelated three-stop network changed"
        assert [e["phase"] for e in last["elevators"]][3:]==["open","closed"],last["elevators"]
    elif mode == "seals":
        result = snapshot("checkpoint:sealed-upper")
        assert result["elevators"][0]["phase"] == "open" and result["elevators"][0]["stop"] == 1, result
        assert len(list(state.glob("*.png"))) == 12
    elif mode == "views":
        from PIL import Image

        for name in ("closed", "open", "interior", "controls"):
            with Image.open(state / f"elevator-{name}.png") as captured:
                assert captured.width >= 1200 and captured.height >= 800
    elif mode in ("failure", "cancel"):
        result = snapshot(f"checkpoint:{mode}_recovered")
        lift = result["elevators"][0]
        assert result["current_level_id"] == "elevator_lobby", result
        assert result["state"] == "Playing" and not result["transition_frozen"], result
        assert lift["phase"] == "open" and lift["opening"] == 1, lift
    elif mode == "flow":
        from PIL import Image, ImageChops

        result = snapshot("checkpoint:return_lobby")
        assert result["current_level_id"] == "elevator_lobby", result
        assert result["elevators"][0]["phase"] == "open", result
        resumed = snapshot("checkpoint:upper_resumed")
        assert resumed["elevators"][0]["stop"] == 1, resumed
        assert abs(resumed["player_position"][1] - 5.1) < 0.002, resumed
        assert abs(snapshot("checkpoint:upper")["player_position"][1] - 5.1) < 0.002
        frozen = snapshot("checkpoint:frozen_responsive")
        assert frozen["transition_frozen"] and frozen["current_level_id"] == "elevator_lobby"
        assert abs(result["player_position"][1] - 1.6) < 0.002, result
        assert 0.4 < result["player_position"][2] < 2.1, result
        with Image.open(state / "frozen_a.png") as first:
            expected_resize = (round(first.width * 1000 / 960), round(first.height * 700 / 640))
            with Image.open(state / "frozen_b.png") as second:
                assert ImageChops.difference(first, second).getbbox() is None
        with Image.open(state / "frozen_resized.png") as resized:
            assert resized.size == expected_resize, resized.size
    else:
        def gain(result):
            return next(voice["gain"] for voice in result["audio_voices"]
                        if voice["cue"] == "elevator_shaft")

        near = snapshot("checkpoint:near_sound")
        far = snapshot("checkpoint:far_sound")
        again = snapshot("checkpoint:near_return")
        assert gain(near) > gain(far) * 3, (near, far)
        assert abs(gain(near) - gain(again)) < 0.001, (near, again)
        for result in (near, far, again):
            assert result["audio"]["enabled"] and result["audio"]["errors"] == 0, result["audio"]
            assert result["audio"]["dropped"] == 0 and result["audio"]["output_frames"] > 0


def run_case(binary, package_source, output, mode, lightmaps="off"):
    out = output / mode
    out.mkdir()
    state = out / "state"
    state.mkdir()
    package = state / "levels/elevator-lab.placesmap"
    package.parent.mkdir()
    shutil.copy2(package_source, package)
    settings = json.loads((ROOT / "tests/fixtures/native/hero-manifest.json").read_text())["settings"]
    settings.update(
        window_width=960, window_height=640, quality="high", texture_filtering="high",
        lightmaps=lightmaps, reflections="off", bloom=False, vsync=True,
        fov_degrees=65, walk_speed=2, ambient_volume=0,
        master_volume=0.75, effects_volume=0.8,
    )
    if mode in ("views", "seals"):
        settings.update(window_width=1200, window_height=800, fov_degrees=75)
    (state / "settings.json").write_text(json.dumps(settings))
    script = out / "actions.json"
    sequence = actions(mode)
    if mode in ("flow", "cancel", "failure"):
        # PNG readback can clamp a simulation step. Walk fully past the new
        # inner door stock before asking it to close around the passenger.
        for step in sequence:
            if step["after"] == LOBBY_READY and step["delay_ms"] >= 3050:
                step["delay_ms"] += 300
    script.write_text(json.dumps(sequence, indent=2) + "\n")
    trace = out / "trace.jsonl"
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    env.update(
        PLACES_ASSET_ROOT=str(ROOT / "assets"), PLACES_STATE_ROOT=str(state),
        PLACES_LEVEL="elevator_groups" if mode == "links" else "elevator_lobby", PLACES_BENCH="1", PLACES_BENCH_FRAMES="6000",
        PLACES_BENCH_WARMUP="3", PLACES_VERBOSE="1", PLACES_PREPARE_DELAY_MS="500",
        PLACES_LOAD_TRACE=str(trace), PLACES_BENCH_ACTIONS=str(script),
        PLACES_BENCH_OUT=str(out / "frames.csv"), PLACES_AUDIO_CAPTURE="1",
    )
    started = time.monotonic()
    unavailable = package.with_suffix(".unavailable")
    removed = False
    with (out / "run.log").open("w") as log:
        proc = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            while proc.poll() is None:
                if time.monotonic() - started > 70:
                    raise RuntimeError(f"native {mode} run exceeded 70 seconds; see {out}")
                if mode == "failure" and not removed:
                    requested = any(row.get("event") == "request"
                                    and row.get("detail") == "elevator_annex"
                                    for row in read_trace(trace))
                    if requested:
                        package.rename(unavailable)
                        removed = True
                time.sleep(0.05)
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5)
            if removed:
                unavailable.rename(package)
    receipt = {
        "mode": mode, "lightmaps": lightmaps, "binary": str(binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "package_sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
        "exit_code": proc.returncode, "wall_seconds": time.monotonic() - started,
        "screenshots": [str(path) for path in sorted(state.glob("*.png"))],
        "trace": str(trace),
    }
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    if proc.returncode:
        raise RuntimeError(f"native {mode} exited {proc.returncode}; see {out / 'run.log'}")
    assert_case(mode, state, read_trace(trace))
    (out / "passed.json").write_text(json.dumps({"mode": mode, "passed": True}) + "\n")
    print(f"Native assertions passed: {mode} ({receipt['wall_seconds']:.1f}s)", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True,
                        help="new evidence directory; existing output is refused")
    parser.add_argument("--mode", choices=["all", "flow", "cancel", "failure", "sound", "views", "seals", "links"], default="all")
    parser.add_argument("--lightmaps", choices=["off", "full"], default="off")
    args = parser.parse_args()
    binary = args.binary.resolve()
    package = args.package.resolve()
    output = args.out.resolve()
    if not binary.is_file() or not package.is_file():
        parser.error("binary and prepared project package must be existing files")
    output.mkdir(parents=True, exist_ok=False)
    modes = ["flow", "cancel", "failure", "sound"] if args.mode == "all" else [args.mode]
    for mode in modes:
        run_case(binary, package, output, mode, args.lightmaps)


if __name__ == "__main__":
    main()

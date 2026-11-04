#!/usr/bin/env python3
"""Explicit direct-binary elevator light captures; no shipped-map bake or CI work.

Use tests/fixtures/levels/elevator_dark.json prepared as Static + Full. Every
capture has an authoritative gameplay checkpoint; no test-only door/light pose.
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


def floor_measurements(state):
    """Fixed image-space patch below the sill; display values, not radiometry."""
    from PIL import Image, ImageStat

    measurements = {}
    for name in ("closed-front", "opening-early", "opening-middle", "open-front",
                 "closing-early", "closing-middle", "closed-departed",
                 "closed-side", "open-side", "closed-rear", "open-rear", "dark-reference"):
        with Image.open(state / (name + ".png")) as image:
            scale_x, scale_y = image.width / 1280, image.height / 720
            box = tuple(round(value * scale) for value, scale in
                        zip((500, 545, 780, 590), (scale_x, scale_y, scale_x, scale_y)))
            patch = image.convert("RGB").crop(box)
            mean = ImageStat.Stat(patch).mean
            measurements[name] = dict(pixel_box=box, mean_display_rgb_255=mean,
                                      max_display_channel=max(high for _, high in patch.getextrema()))
    return measurements


def route():
    steps = []
    anchor = "ready:elevator_dark"

    def add(ms, kind, **fields):
        steps.append(dict(after=anchor, delay_ms=ms, action=dict(kind=kind, **fields)))

    def pose(ms, position, target):
        add(ms, "player", position=position, yaw=180, pitch=0)
        add(ms + 20, "view_at", position=target)

    def capture(ms, name):
        nonlocal anchor
        add(ms, "checkpoint", name=name)
        add(ms + 20, "capture", name=name)
        # PNG readback may stall a frame. Anchor the next group after this
        # capture has finished instead of allowing overdue poses to bunch up.
        add(ms + 450, "checkpoint", name=name + "_done")
        anchor = "checkpoint:" + name + "_done"

    def click(ms):
        add(ms, "mouse", button="left", pressed=True)
        add(ms + 70, "mouse", button="left", pressed=False)

    views = [
        ("front", [0, 1.6, -2.5], [0, .9, .1]),
        ("left", [-1.7, 1.6, -1.5], [0, .8, .1]),
        ("right", [1.7, 1.6, -1.5], [0, .8, .1]),
        ("inside", [0, 1.6, 1.9], [0, 1.1, .2]),
        ("header", [0, 1.6, 1.8], [0, 2.4, .36]),
        ("side", [2.1, 1.6, 1.25], [1.3, .5, 1.2]),
        ("rear", [0, 1.6, 3.3], [0, .5, 2.4]),
    ]
    for name, position, target in views:
        pose(100, position, target)
        capture(280, "closed-" + name)
    pose(100, [-.96, 1.6, -.9], [-.96, 1.08, -.12])
    click(300)
    pose(400, views[0][1], views[0][2])
    capture(650, "opening-early")
    capture(200, "opening-middle")
    capture(200, "opening-late")
    for name, position, target in views:
        pose(100, position, target)
        capture(1800 if name == "front" else 280, "open-" + name)
    pose(100, [0, 1.6, 1.2], [.94, 1.58, .4])
    click(300)
    pose(400, views[0][1], views[0][2])
    capture(650, "closing-early")
    capture(200, "closing-middle")
    capture(200, "closing-late")
    capture(1000, "closed-departed")
    capture(6500, "dark-reference")
    pose(100, [0, 5.1, -2.5], [0, 4.4, .1])
    capture(280, "upper-open")
    add(100, "quit")
    return steps


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--lightmaps", choices=["off", "full"], default="full")
    parser.add_argument("--bloom", action="store_true")
    parser.add_argument("--assert-sealed", action="store_true",
                        help="Require zero cabin spill in the dark fixture's exterior floor patches")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    package = args.package.resolve(strict=True)
    out = args.out.resolve()
    state = out / "state"
    levels = state / "levels"
    levels.mkdir(parents=True)
    shutil.copy2(package, levels / "elevator-dark.placesmap")
    settings = json.loads((ROOT / "tests/fixtures/native/hero-manifest.json").read_text())["settings"]
    settings.update(window_width=640, window_height=360, quality="high",
                    texture_filtering="high", lightmaps=args.lightmaps, reflections="off",
                    bloom=args.bloom, vsync=True, fov_degrees=75, ambient_volume=0,
                    master_volume=0)
    (state / "settings.json").write_text(json.dumps(settings))
    actions = out / "actions.json"
    actions.write_text(json.dumps(route()))
    trace = out / "trace.jsonl"
    env = {k: v for k, v in os.environ.items() if not k.startswith("PLACES_")}
    env.update(PLACES_ASSET_ROOT=str(ROOT / "assets"), PLACES_STATE_ROOT=str(state),
               PLACES_LEVEL="elevator_dark", PLACES_BENCH="1", PLACES_BENCH_FRAMES="6000",
               PLACES_BENCH_WARMUP="3", PLACES_VERBOSE="1", PLACES_LOAD_TRACE=str(trace),
               PLACES_BENCH_ACTIONS=str(actions), PLACES_BENCH_OUT=str(out / "frames.csv"))
    started = time.monotonic()
    with (out / "game.log").open("w") as log:
        result = subprocess.run([str(binary)], cwd=ROOT, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=90, check=True)
    rows = [json.loads(line) for line in trace.read_text().splitlines()]
    states = {row["event"].removeprefix("checkpoint:"): json.loads(row["detail"])
              for row in rows if row["event"].startswith("checkpoint:")}
    receipt = dict(binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                   package_sha256=hashlib.sha256(package.read_bytes()).hexdigest(),
                   lightmaps=args.lightmaps, bloom=args.bloom, exit_code=result.returncode,
                   wall_seconds=time.monotonic() - started, checkpoints=states,
                   floor_measurements=floor_measurements(state),
                   screenshots=[str(p) for p in sorted(state.glob("*.png"))])
    (out / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    assert len(receipt["screenshots"]) == 23, receipt["screenshots"]
    assert states["closed-front"]["elevators"][0]["opening"] == 0
    assert states["open-front"]["elevators"][0]["opening"] == 1
    assert states["closed-departed"]["elevators"][0]["opening"] == 0
    assert states["upper-open"]["elevators"][0]["stop"] == 1
    assert states["upper-open"]["elevators"][0]["opening"] == 1
    if args.assert_sealed:
        from PIL import Image, ImageChops

        def patch(name):
            with Image.open(state / (name + ".png")) as image:
                return image.convert("RGB").crop(receipt["floor_measurements"][name]["pixel_box"])

        # Static retains ordinary ambient illumination. Compare against the
        # same floor with the source upstairs and beyond its entire radius.
        reference = patch("dark-reference")
        for name in ("closed-front", "closed-departed"):
            assert ImageChops.difference(patch(name), reference).getbbox() is None, name
        for closed, opened in (("closed-side", "open-side"), ("closed-rear", "open-rear")):
            assert ImageChops.difference(patch(closed), patch(opened)).getbbox() is None, closed
        means = receipt["floor_measurements"]
        assert means["open-front"]["mean_display_rgb_255"][0] > means["dark-reference"]["mean_display_rgb_255"][0] + 0.05
    print(json.dumps({k: v for k, v in receipt.items()
                      if k not in ("checkpoints", "screenshots", "floor_measurements")}))


if __name__ == "__main__":
    main()

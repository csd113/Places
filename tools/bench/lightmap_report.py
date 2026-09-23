#!/usr/bin/env python3
"""Capture and report the lighting/bake numbers one build produces.

This is the measurement half of the lightmap validation: it drives the
one-frame capture path (`LIMINAL_CAPTURE`) and the benchmark telemetry
(`LIMINAL_BENCH=1`) over a fixed shot list, records every developer log line the
run printed, and writes a machine-readable report next to the PNGs.

Nothing here changes the engine. It exists so a before/after claim about bake
time, lightmap memory, draw calls or frame time is a number in a file rather
than an impression.

Usage:
    python3 tools/bench/lightmap_report.py --binary target/release/liminal-rust \
        --label full --out target/agent-work/benchmarks/full

    # the vertex-lit control run
    LIMINAL_NO_LIGHTMAPS=1 python3 tools/bench/lightmap_report.py \
        --binary target/release/liminal-rust --label vertex \
        --out target/agent-work/benchmarks/vertex
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
WORK = REPO / "target" / "agent-work" / "benchmarks"

# One shot per level/camera state worth measuring. The fixtures are staged into
# the run directory's `levels/` link, exactly as the visual check does.
SHOTS: list[tuple[str, str, dict[str, str]]] = [
    ("demo_spawn", "places_demo", {}),
    ("demo_pool", "places_demo", {"LIMINAL_SPAWN": "3.5,9.0,0"}),
    ("demo_far_corridor", "places_demo", {"LIMINAL_SPAWN": "40.0,13.0,90"}),
    ("demo_office", "places_demo", {"LIMINAL_SPAWN": "4.6,5.8,0"}),
    # Close-ups of prop/floor and prop/wall contact, the surfaces the baked
    # occlusion has to ground. Each looks slightly down at a placed prop.
    ("demo_desk_contact", "places_demo", {"LIMINAL_SPAWN": "3.6,4.6,140", "LIMINAL_CAMERA": "140,-18"}),
    ("demo_cabinet_contact", "places_demo", {"LIMINAL_SPAWN": "8.5,2.9,0", "LIMINAL_CAMERA": "0,-16"}),
    ("demo_pool_table", "places_demo", {"LIMINAL_SPAWN": "4.0,13.6,180", "LIMINAL_CAMERA": "180,-18"}),
    ("demo_doorway", "places_demo", {"LIMINAL_SPAWN": "9.0,3.5,0"}),
    ("prop_stress", "prop_stress", {}),
    ("prop_stress_close", "prop_stress", {"LIMINAL_SPAWN": "2.0,2.0,135", "LIMINAL_CAMERA": "135,-16"}),
    ("prop_showcase", "prop_showcase", {}),
    ("test_room", "test_room", {}),
    ("lighting_diagnostic", "lighting_diagnostic", {}),
    ("lighting_isolation", "lighting_isolation", {}),
]

# Developer log lines worth keeping, by their `[tag]` prefix.
LOG_TAGS = ("level", "lighting", "lightmap", "spatial", "props", "fixtures", "decals")

NUMBER = r"(-?\d+(?:\.\d+)?)"


def run_shot(binary: Path, workdir: Path, level: str, env: dict[str, str], out_png: Path,
             frames: int, extra: dict[str, str]) -> tuple[int, str]:
    """Renders one frame (or runs a short benchmark) and returns its log."""
    env = dict(env)
    env.setdefault("LIMINAL_LEVEL", level)
    env.setdefault("LIMINAL_BENCH", "1")
    env.setdefault("LIMINAL_BENCH_WARMUP", "2")
    if frames > 1:
        env.setdefault("LIMINAL_BENCH_OUT", str(out_png.with_suffix(".csv")))
    else:
        env.setdefault("LIMINAL_CAPTURE", str(out_png))
    env.update(extra)
    env.setdefault("PATH", os.environ.get("PATH", ""))
    proc = subprocess.run(
        [str(binary)], cwd=workdir, env=env, capture_output=True, text=True, check=False
    )
    return proc.returncode, proc.stdout + proc.stderr


def parse_logs(text: str) -> dict[str, object]:
    """Extracts the numbers the developer log already prints.

    A run loads the default level first and then the requested one, so every
    pattern takes its *last* match: the numbers reported are the requested
    level's, not the menu's first boot.
    """
    out: dict[str, object] = {"lines": {}}
    for line in text.splitlines():
        if not line.startswith("["):
            continue
        tag = line.split("]", 1)[0].lstrip("[")
        out["lines"].setdefault(tag, []).append(line)

    def last(pattern: str, tag: str) -> re.Match[str] | None:
        found = None
        for line in out["lines"].get(tag, []):
            for match in re.finditer(pattern, str(line)):
                found = match
        return found

    joined = "\n".join(str(line) for line in out["lines"].get("level", []))
    m = last(r"(\d+) static vertices, (\d+) prop vertices", "level")
    if m:
        out["static_vertices"] = int(m.group(1))
        out["prop_vertices"] = int(m.group(2))
    # Durations take the *largest* value in the run: the first load bakes, the
    # LIMINAL_LEVEL load is the cache hit. Counts take the last, which is the
    # requested level.
    durations: dict[int, list[float]] = {}
    for line in out["lines"].get("level", []):
        match = re.search(
            rf"built in {NUMBER} ms \(lighting {NUMBER} \+ props {NUMBER} \+ surfaces {NUMBER}\)",
            str(line),
        )
        if match:
            for slot in range(4):
                durations.setdefault(slot, []).append(float(match.group(slot + 1)))
    for key, slot in (("build_ms", 0), ("lighting_ms", 1), ("props_ms", 2), ("surfaces_ms", 3)):
        values = durations.get(slot)
        if values:
            out[key] = max(values)

    m = last(r"(\d+) static batch\(es\)", "spatial")
    if m:
        out["static_batches"] = int(m.group(1))
    m = last(r"(\d+) prop batch\(es\)", "spatial")
    if m:
        out["prop_batches"] = int(m.group(1))

    # The reported bake time is the largest in the run: a level loads once from
    # the menu and once from LIMINAL_LEVEL, and the second load is a cache hit
    # (0 ms). `--cold` removes the cache first so this is a real bake.
    for key, pattern in (
        ("lightmap_pages", r"(\d+) page\(s\)"),
        ("lightmap_charts", r"(\d+) chart\(s\)"),
        ("lightmap_chart_texels", r"(\d+) chart texels"),
        ("lightmap_texels", r"(\d+) page texels"),
        ("lightmap_kib", r"\((\d+) KiB\)"),
        ("lightmap_bake_ms", r"filled in " + NUMBER + " ms"),
    ):
        found = [
            float(match.group(1))
            for line in out["lines"].get("lightmaps", [])
            for match in re.finditer(pattern, str(line))
        ]
        if found:
            value = max(found) if key == "lightmap_bake_ms" else found[-1]
            out[key] = int(value) if value.is_integer() else value
    if last(r"fallback: vertex lighting", "lightmaps"):
        out["lightmap_fallback"] = 1

    m = last(r"baked (\d+) room", "lighting")
    if m:
        out["rooms"] = int(m.group(1))
    m = last(r"(\d+) wall \+ (\d+) slab \+ (\d+) prop blocker", "lighting")
    if m:
        out["wall_blockers"] = int(m.group(1))
        out["slab_blockers"] = int(m.group(2))
        out["prop_blockers"] = int(m.group(3))
    m = last(rf"baselines {NUMBER}\.\.{NUMBER} \(avg {NUMBER}\)", "lighting")
    if m:
        out["baseline_min"] = float(m.group(1))
        out["baseline_max"] = float(m.group(2))
        out["baseline_avg"] = float(m.group(3))
    del joined
    return out


def parse_bench_csv(path: Path) -> dict[str, float]:
    if not path.is_file():
        return {}
    rows = path.read_text().strip().splitlines()
    if len(rows) < 2:
        return {}
    header = rows[0].split(",")
    data = [dict(zip(header, row.split(","))) for row in rows[1:]]
    out: dict[str, float] = {"frames": float(len(data))}
    for column in ("update_ms", "render_ms", "frame_ms", "loop_ms", "draw_calls",
                   "visible_vertices", "total_batches"):
        values = [float(row[column]) for row in data if column in row]
        if values:
            values.sort()
            out[f"{column}_median"] = values[len(values) // 2]
            out[f"{column}_min"] = values[0]
            out[f"{column}_max"] = values[-1]
    return out


def stage_run_dir(directory: Path) -> None:
    """A package root the game can boot from: the real assets plus the fixture levels.

    The binary resolves its package root through `LIMINAL_ASSET_ROOT`, then
    changes into it, so `assets/` and `levels/` are reached through this
    directory and nothing temporary has to be copied into the repository's own
    `levels/`.
    """
    directory.mkdir(parents=True, exist_ok=True)
    for name, target in (
        ("assets", REPO / "assets"),
        ("levels", REPO / "tests" / "fixtures" / "levels"),
    ):
        link = directory / name
        if link.is_symlink():
            link.unlink()
        elif link.exists():
            continue
        link.symlink_to(target, target_is_directory=True)


def run_shot(binary: Path, workdir: Path, level: str, env: dict[str, str], out_png: Path,
             frames: int, extra: dict[str, str]) -> tuple[int, str]:
    """Renders one frame (or runs a short benchmark) and returns its log."""
    env = dict(env)
    env.setdefault("LIMINAL_ASSET_ROOT", str(workdir))
    env.setdefault("LIMINAL_LEVEL", level)
    env.setdefault("LIMINAL_BENCH", "1")
    env.setdefault("LIMINAL_BENCH_WARMUP", "2")
    # The capture is read back from the framebuffer *before* the swap, so
    # skipping the swap loses nothing and keeps a run from blocking on a window
    # server that is not completing presents (a headless or locked display).
    env.setdefault("LIMINAL_BENCH_NOSWAP", "1")
    if frames > 1:
        env.setdefault("LIMINAL_BENCH_FRAMES", str(frames))
        env.setdefault("LIMINAL_BENCH_OUT", str(out_png.with_suffix(".csv")))
    else:
        env.setdefault("LIMINAL_CAPTURE", str(out_png))
    env.update(extra)
    env.setdefault("PATH", os.environ.get("PATH", ""))
    proc = subprocess.run(
        [str(binary)], cwd=workdir, env=env, capture_output=True, text=True, check=False
    )
    return proc.returncode, proc.stdout + proc.stderr


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=str(REPO / "target" / "release" / "liminal-rust"))
    parser.add_argument("--label", default="run")
    parser.add_argument("--out", default=None)
    parser.add_argument("--frames", type=int, default=1,
                        help="1 = capture a PNG; >1 = also record frame timings")
    parser.add_argument("--shots", default="all", help="comma-separated shot names, or all")
    parser.add_argument("--env", action="append", default=[],
                        help="extra KEY=VALUE for every shot")
    parser.add_argument("--cold", action="store_true",
                        help="delete the run directory's lightmap cache first")
    parser.add_argument("--run-dir", default=None,
                        help="working directory to run from (default: a staged dir "
                             "under target/agent-work/benchmarks)")
    args = parser.parse_args()

    binary = Path(args.binary).resolve()
    if not binary.is_file():
        print(f"no such binary: {binary}", file=sys.stderr)
        return 2
    out_dir = Path(args.out) if args.out else WORK / args.label
    out_dir.mkdir(parents=True, exist_ok=True)
    run_dir = Path(args.run_dir) if args.run_dir else WORK / f"run-{args.label}"
    stage_run_dir(run_dir)
    if args.cold:
        cache = run_dir / "target" / "level-cache" / "lightmaps"
        if cache.is_dir():
            for entry in sorted(cache.iterdir()):
                if entry.is_dir():
                    for child in entry.iterdir():
                        child.unlink()
                    entry.rmdir()
            cache.rmdir()
            print(f"cleared {cache}")

    extra: dict[str, str] = {}
    for item in args.env:
        key, _, value = item.partition("=")
        extra[key] = value

    wanted = {name.strip() for name in args.shots.split(",") if name.strip()}
    report: dict[str, object] = {"label": args.label, "binary": str(binary), "shots": {}}
    failures = 0
    for name, level, shot_env in SHOTS:
        if wanted and "all" not in wanted and name not in wanted:
            continue
        png = out_dir / f"{name}.png"
        code, log = run_shot(binary, run_dir, level, shot_env, png, args.frames, extra)
        parsed = parse_logs(log)
        parsed["exit_code"] = code
        parsed["png"] = png.name
        parsed["png_bytes"] = png.stat().st_size if png.is_file() else 0
        parsed.update(parse_bench_csv(png.with_suffix(".csv")))
        (out_dir / f"{name}.log").write_text(log)
        report["shots"][name] = parsed
        if code != 0:
            failures += 1
        print(f"{name:22} exit={code} build={parsed.get('build_ms', '?')}ms "
              f"static={parsed.get('static_vertices', '?')} v "
              f"lm={parsed.get('lightmap_pages', '-')}p/"
              f"{parsed.get('lightmap_texels', '-')}tx")

    (out_dir / "report.json").write_text(json.dumps(report, indent=2, sort_keys=True))
    print(f"wrote {out_dir / 'report.json'}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())

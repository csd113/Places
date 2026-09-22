#!/usr/bin/env python3
"""Drive a PocketCHIP benchmark suite over SSH.

The device does the measuring; this script only stages the payload and collects
the results, so the numbers come from the real hardware rather than from a
desktop approximation.

Layout (all inside `/tmp/liminal-benchmark`, removed with `--cleanup`):

```text
/tmp/liminal-benchmark/liminal-rust     release binary for armv7
/tmp/liminal-benchmark/assets/          shipped assets (catalog, demo, props)
/tmp/liminal-benchmark/levels/          demo fixtures + generated bench levels
/tmp/liminal-benchmark/bench_suite.sh   generated per-phase runner
/tmp/liminal-benchmark/out/*.csv        per-frame samples written by the game
/tmp/liminal-benchmark/out/*.log        the game's stdout for each run
```

Results land in `tools/bench/results/<phase>/`.

Nothing outside the temporary directory is written: the game keeps its
`settings.json` in the working directory, and `XDG_*` point inside the
temporary tree too.
"""

from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
EXPECT_SSH = Path(
    "/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/chip-ssh.exp"
)
EXPECT_SCP = Path(
    "/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/chip-scp.exp"
)
REMOTE_ROOT = "/tmp/liminal-benchmark"
BINARY = REPO / "target/armv7-unknown-linux-gnueabihf/release/liminal-rust"

# Default frame budget per run: 60 warm-up frames then 600 measured frames is
# about 10 s at 60 FPS, which is long enough for a stable median and short
# enough that a 30-run suite stays under ten minutes.
DEFAULT_WARMUP = 60
DEFAULT_FRAMES = 600

# Scene sets. Each entry is (label suffix, level id, extra environment). The
# special environment key `BIN` selects which uploaded build to run:
#   `current` -> ./liminal-rust        (the build under test)
#   `phase1`  -> ./liminal-rust-phase1 (the pre-optimisation baseline)
SCENE_SETS: dict[str, list[tuple[str, str, dict[str, str]]]] = {
    "chairs": [
        (f"chairs_{n}", f"bench_chairs_{n}", {})
        for n in (0, 25, 50, 75, 100, 150, 200, 300, 400, 500, 750, 1000)
    ],
    "chairs_core": [
        (f"chairs_{n}", f"bench_chairs_{n}", {})
        for n in (0, 100, 200, 300, 400, 500)
    ],
    "away": [
        # Camera pinned: yaw 180 looks straight at the chair grid, yaw 0 looks
        # at the near wall with the whole grid behind the camera.
        ("400_facing", "bench_chairs_400", {"LIMINAL_CAMERA": "180"}),
        ("400_away", "bench_chairs_400", {"LIMINAL_CAMERA": "0"}),
        ("400_side", "bench_chairs_400", {"LIMINAL_CAMERA": "90"}),
    ],
    "levels": [
        ("places_demo", "places_demo", {}),
        ("prop_stress", "prop_stress", {}),
        ("prop_showcase", "prop_showcase", {}),
    ],
    # Phase 1: separate the renderer's own cost from the presentation path.
    "vsync": [
        ("swap_only", "bench_chairs_0", {"LIMINAL_BENCH_NORENDER": "1"}),
        ("render_only", "bench_chairs_0", {"LIMINAL_BENCH_NOSWAP": "1"}),
        ("finish", "bench_chairs_0", {"LIMINAL_BENCH_FINISH": "1"}),
        ("normal", "bench_chairs_0", {}),
        ("normal_novsync", "bench_chairs_0", {"LIMINAL_VSYNC": "off"}),
        ("finish_novsync", "bench_chairs_0", {"LIMINAL_BENCH_FINISH": "1", "LIMINAL_VSYNC": "off"}),
        ("heavy_swap_only", "bench_chairs_400", {"LIMINAL_BENCH_NORENDER": "1", "LIMINAL_CAMERA": "180"}),
        ("heavy_render_only", "bench_chairs_400", {"LIMINAL_BENCH_NOSWAP": "1", "LIMINAL_CAMERA": "180"}),
        ("heavy_finish", "bench_chairs_400", {"LIMINAL_BENCH_FINISH": "1", "LIMINAL_CAMERA": "180"}),
        ("heavy_normal", "bench_chairs_400", {"LIMINAL_CAMERA": "180"}),
    ],
    # Phase 2: the same build with culling on and off isolates what culling is
    # worth; the phase 1 build with the same vertex counts isolates what the
    # finer batching costs in draw calls.
    "cull_ab": [
        ("0_cull", "bench_chairs_0", {}),
        ("0_nocull", "bench_chairs_0", {"LIMINAL_BENCH_NOCULL": "1"}),
        ("400_facing_cull", "bench_chairs_400", {"LIMINAL_CAMERA": "180"}),
        ("400_facing_nocull", "bench_chairs_400", {"LIMINAL_CAMERA": "180", "LIMINAL_BENCH_NOCULL": "1"}),
        ("400_away_cull", "bench_chairs_400", {"LIMINAL_CAMERA": "0"}),
        ("400_away_nocull", "bench_chairs_400", {"LIMINAL_CAMERA": "0", "LIMINAL_BENCH_NOCULL": "1"}),
        ("1000_facing_cull", "bench_chairs_1000", {"LIMINAL_CAMERA": "180"}),
        ("1000_facing_nocull", "bench_chairs_1000", {"LIMINAL_CAMERA": "180", "LIMINAL_BENCH_NOCULL": "1"}),
        ("400_facing_phase1", "bench_chairs_400", {"LIMINAL_CAMERA": "180", "BIN": "phase1"}),
        ("400_away_phase1", "bench_chairs_400", {"LIMINAL_CAMERA": "0", "BIN": "phase1"}),
    ],
    # The four renderer phases on one build, each switch changing exactly one
    # submission decision with the level build, batching, draw order and shader
    # held fixed. `phase1_pre` is the pre-optimisation submission shape.
    # Every variant of a scene shares the same camera, so only the switch differs.
    "phase_ab": [
        (f"{label}_{variant}", level, dict(camera) | dict(extra))
        for label, level, camera in [
            ("chairs400_facing", "bench_chairs_400", {"LIMINAL_CAMERA": "180"}),
            ("chairs400_away", "bench_chairs_400", {"LIMINAL_CAMERA": "0"}),
            ("chairs1000_facing", "bench_chairs_1000", {"LIMINAL_CAMERA": "180"}),
            ("places_demo", "places_demo", {}),
            ("prop_stress", "prop_stress", {}),
            ("prop_showcase", "prop_showcase", {}),
        ]
        for variant, extra in [
            # Exactly the shipping build: culling + indexing + packed vertices.
            ("phase4", {}),
            # Culling + indexing, exact 36-byte vertices: Phase 3.
            ("phase3", {"LIMINAL_BENCH_EXACT_VERTEX": "1"}),
            # Culling only, flat non-indexed 36-byte vertices: Phase 2.
            (
                "phase2",
                {
                    "LIMINAL_BENCH_NOINDEX": "1",
                    "LIMINAL_BENCH_EXACT_VERTEX": "1",
                },
            ),
            # Pre-optimisation submission shape: Phase 1.
            (
                "phase1",
                {
                    "LIMINAL_BENCH_NOCULL": "1",
                    "LIMINAL_BENCH_NOINDEX": "1",
                    "LIMINAL_BENCH_EXACT_VERTEX": "1",
                },
            ),
            # Single-variable diagnostics for the culling and indexing deltas.
            ("nocull", {"LIMINAL_BENCH_NOCULL": "1"}),
            ("noindex", {"LIMINAL_BENCH_NOINDEX": "1"}),
        ]
    ],
    # Grid-resolution sweep: the draw-call/granularity trade-off, measured.
    "cellsweep": [
        (f"400_facing_cell{size}", "bench_chairs_400",
         {"LIMINAL_CAMERA": "180", "LIMINAL_CELL_METRES": str(size)})
        for size in (12, 18, 24, 36, 48, 96)
    ]
    + [
        (f"400_away_cell{size}", "bench_chairs_400",
         {"LIMINAL_CAMERA": "0", "LIMINAL_CELL_METRES": str(size)})
        for size in (12, 24, 48, 96)
    ],
    "levels_ab": [
        ("places_demo_cull", "places_demo", {}),
        ("places_demo_nocull", "places_demo", {"LIMINAL_BENCH_NOCULL": "1"}),
        ("places_demo_phase1", "places_demo", {"BIN": "phase1"}),
        ("prop_stress_cull", "prop_stress", {}),
        ("prop_stress_nocull", "prop_stress", {"LIMINAL_BENCH_NOCULL": "1"}),
        ("prop_stress_phase1", "prop_stress", {"BIN": "phase1"}),
        ("prop_showcase_cull", "prop_showcase", {}),
        ("prop_showcase_nocull", "prop_showcase", {"LIMINAL_BENCH_NOCULL": "1"}),
        ("prop_showcase_phase1", "prop_showcase", {"BIN": "phase1"}),
    ],
}

# Uploaded build names, in the order the deploy step copies them.
BINARIES = {
    "current": "liminal-rust",
    "phase1": "liminal-rust-phase1",
}


def run(cmd: list[str], **kwargs) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=False, text=True, **kwargs)


def ssh(host: str, command: str, timeout: int = 3600) -> subprocess.CompletedProcess:
    return run([str(EXPECT_SSH), host, command], capture_output=True, timeout=timeout)


def upload(host: str, local: Path, remote_dir: str) -> None:
    result = run(
        [str(EXPECT_SCP), host, str(local), remote_dir + "/"],
        capture_output=True,
        timeout=1800,
    )
    if "100%" not in result.stdout and result.returncode != 0:
        raise SystemExit(f"upload of {local} failed: {result.stdout}\n{result.stderr}")


def upload_binary(host: str, local: Path, remote_name: str) -> None:
    """Uploads a local binary as a specific name inside the temp directory."""
    staging = Path(
        "/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/stage"
    )
    staging.mkdir(parents=True, exist_ok=True)
    target = staging / remote_name
    target.write_bytes(local.read_bytes())
    upload(host, target, REMOTE_ROOT)
    ssh(host, f"chmod +x {REMOTE_ROOT}/{remote_name}")


def build_suite_script(
    phase: str,
    scenes: list[tuple[str, str, dict[str, str]]],
    repeats: int,
    warmup: int,
    frames: int,
    global_env: dict[str, str],
) -> str:
    """Generates the device-side runner: one line per run, sequential."""
    lines = [
        "#!/bin/sh",
        "# Generated by tools/bench/run_bench.py -- do not edit on the device.",
        "set -u",
        f"ROOT={shlex.quote(REMOTE_ROOT)}",
        'cd "$ROOT" || exit 1',
        "mkdir -p out",
        f'rm -f out/{phase}__*.csv out/{phase}__*.log',
        'export DISPLAY=:0',
        'export XAUTHORITY="$HOME/.Xauthority"',
        'export XDG_DATA_HOME="$ROOT/.xdg/data"',
        'export XDG_CONFIG_HOME="$ROOT/.xdg/config"',
        'export XDG_CACHE_HOME="$ROOT/.xdg/cache"',
        'mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"',
        "export LIMINAL_BENCH=1",
        f"export LIMINAL_BENCH_WARMUP={warmup}",
        f"export LIMINAL_BENCH_FRAMES={frames}",
    ]
    for key, value in global_env.items():
        lines.append(f"export {key}={shlex.quote(value)}")
    lines.append("")
    for repeat in range(1, repeats + 1):
        for label, level, env in scenes:
            name = f"{phase}__{label}__r{repeat}"
            env = dict(env)
            binary = BINARIES[env.pop("BIN", "current")]
            assign = " ".join(f"{key}={shlex.quote(value)}" for key, value in env.items())
            lines.append(f"echo '=== {name} ===' | tee \"out/{name}.log\"")
            lines.append(
                f"env LIMINAL_LEVEL={shlex.quote(level)} LIMINAL_BENCH_OUT=\"$ROOT/out/{name}.csv\" "
                f"{assign} ./{binary} 2>&1 | tee -a \"out/{name}.log\" | grep -E 'BENCH_SUMMARY|\\[vsync\\]|\\[level\\]|\\[spatial\\]' | tail -4"
            )
    lines.append("")
    lines.append('echo "SUITE_DONE"')
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="10.0.0.137")
    parser.add_argument("--phase", required=True, help="label prefix for the results directory")
    parser.add_argument(
        "--scenes",
        default="chairs_core",
        help=f"comma-separated scene sets: {', '.join(SCENE_SETS)}",
    )
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--warmup", type=int, default=DEFAULT_WARMUP)
    parser.add_argument("--frames", type=int, default=DEFAULT_FRAMES)
    parser.add_argument(
        "--env",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="extra environment for every run (repeatable)",
    )
    parser.add_argument("--no-deploy", action="store_true", help="skip uploading the payload")
    parser.add_argument("--no-build", action="store_true", help="skip the cross-compile step")
    parser.add_argument("--cleanup", action="store_true", help="delete the device temp dir and exit")
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print the device-side suite script and exit (touches nothing)",
    )
    args = parser.parse_args()

    if args.cleanup:
        # `~/.local/share/liminal-bench` is left over from an early version of
        # `runone.sh`; nothing else outside the temp directory was ever written.
        command = (
            f"rm -rf {REMOTE_ROOT} ~/.local/share/liminal-bench && "
            f"echo CLEANED; ls -d {REMOTE_ROOT} 2>/dev/null || echo 'temp dir gone'"
        )
        result = ssh(args.host, command)
        print(result.stdout.strip() or result.stderr.strip())
        return

    if args.dry_run:
        for name in args.scenes.split(","):
            for entry in SCENE_SETS[name.strip()]:
                print(entry)
        return

    scenes: list[tuple[str, str, dict[str, str]]] = []
    for name in args.scenes.split(","):
        name = name.strip()
        if name not in SCENE_SETS:
            raise SystemExit(f"unknown scene set '{name}'; have {sorted(SCENE_SETS)}")
        scenes.extend(SCENE_SETS[name])

    global_env: dict[str, str] = {}
    for item in args.env:
        key, _, value = item.partition("=")
        global_env[key.strip()] = value.strip()

    if not args.no_build:
        env = {
            "PKG_CONFIG": str(Path.home() / ".cache/liminal-armhf/pkg-config"),
            "PKG_CONFIG_ALLOW_CROSS": "1",
        }
        print("cross-compiling release build...")
        result = subprocess.run(
            ["cargo", "zigbuild", "--release", "--target", "armv7-unknown-linux-gnueabihf"],
            cwd=REPO,
            env={**os.environ, **env},
        )
        if result.returncode != 0:
            raise SystemExit("cross-compile failed")

    if not args.no_deploy:
        print("staging payload...")
        staging = Path("/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/stage")
        (staging / "levels").mkdir(parents=True, exist_ok=True)
        ssh(args.host, f"mkdir -p {REMOTE_ROOT}")
        upload_binary(args.host, BINARY, "liminal-rust")
        baseline = REPO / "target/phase1/liminal-rust"
        if baseline.exists():
            upload_binary(args.host, baseline, "liminal-rust-phase1")
        else:
            print(f"note: no phase-1 baseline at {baseline}; BIN=phase1 scenes will fail")
        upload(args.host, REPO / "assets", REMOTE_ROOT)
        # Regenerate the deterministic bench levels so the device copy can never
        # drift from the generator.
        subprocess.run(
            [sys.executable, str(HERE / "gen_levels.py"), "--out", str(staging / "levels")],
            check=True,
            capture_output=True,
        )
        for level in (REPO / "levels").glob("*.json"):
            (staging / "levels" / level.name).write_bytes(level.read_bytes())
        # The prop fixtures live outside the shipped content but are measured
        # levels too; stage them as ordinary drop-in levels on the device.
        for name in ("prop_showcase", "prop_stress"):
            level = REPO / "tests" / "fixtures" / "levels" / f"{name}.json"
            (staging / "levels" / level.name).write_bytes(level.read_bytes())
        upload(args.host, staging / "levels", REMOTE_ROOT)

    script = build_suite_script(
        args.phase, scenes, args.repeat, args.warmup, args.frames, global_env
    )
    script_path = Path(
        "/private/var/folders/sl/yxxj_m9n37sgntkntqw0x8yw0000gn/T/opencode/stage/bench_suite.sh"
    )
    script_path.write_text(script)
    upload(args.host, script_path, REMOTE_ROOT)

    print(f"running {len(scenes) * args.repeat} benchmark run(s) on {args.host}...")
    result = ssh(args.host, f"chmod +x {REMOTE_ROOT}/bench_suite.sh && {REMOTE_ROOT}/bench_suite.sh")
    tail = "\n".join(result.stdout.strip().splitlines()[-40:])
    print(tail)
    if "SUITE_DONE" not in result.stdout:
        print("suite did not report SUITE_DONE", file=sys.stderr)
        print(result.stderr.strip()[-2000:], file=sys.stderr)
        raise SystemExit(1)

    results = HERE / "results" / args.phase
    out_dir = results / "out"
    out_dir.mkdir(parents=True, exist_ok=True)
    for stale in out_dir.glob("*"):
        stale.unlink()
    print(f"downloading results to {out_dir}...")
    result = run(
        [
            str(EXPECT_SCP),
            args.host,
            f"{REMOTE_ROOT}/out/{args.phase}__*",
            str(out_dir) + "/",
        ],
        capture_output=True,
        timeout=1800,
    )
    print(result.stderr.strip()[-400:])

    print()
    subprocess.run(
        [
            sys.executable,
            str(HERE / "analyze.py"),
            str(results),
            "--csv",
            str(results / "summary.csv"),
            "--json",
            str(results / "summary.json"),
        ],
        check=False,
    )


if __name__ == "__main__":
    main()

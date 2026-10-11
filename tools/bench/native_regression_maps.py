#!/usr/bin/env python3
"""Run the existing capture tool serially for every supported campaign source.

No build, synthetic image, or source mutation occurs. --geometry instead runs
the player's existing CPU checker, retaining named negative controls. Run only
after receiving the shared native/CPU process allocation.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
EXCLUDED = {"loader-negative", "cpu-boundary-witness", "package-only"}
EXPECTED_ERRORS = {
    "tests/fixtures/levels/geometry_broken.json": {
        "opening-overlap", "collision-duplicate", "duplicate-surface"},
    "tests/fixtures/levels/repair/wall_step_x.json": {"wall-joint-step"},
    "tests/fixtures/levels/repair/wall_step_z.json": {"wall-joint-step"},
    "tests/fixtures/levels/invalid/geometry_invalid.json": {"level-invalid", "curved-invalid"},
}
EXPECTED_WARNINGS = {
    "tests/fixtures/levels/geometry_broken.json": {
        "opening-unused", "missing-wall", "room-leak", "curve-coarse", "spawn-outside-room"},
    "tests/fixtures/levels/repair/gap_review.json": {"wall-joint-step-review"},
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def write_new(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def plan(campaign: Path, manifests: Path, root: Path = ROOT) -> list[dict]:
    inventory = json.loads((campaign / "inventory.json").read_text())["maps"]
    commands = {row["key"]: row["commands"] for row in json.loads((campaign / "plan.json").read_text())}
    receipt = json.loads((campaign / "package-results.json").read_text())
    if not receipt.get("compiler_stable") or receipt.get("failures"):
        raise ValueError("Package campaign must finish with a stable compiler and no failures")
    results = {row["key"]: row["status"] for row in receipt["cases"]}
    supported = [row for row in inventory if row["classification"] not in EXCLUDED]
    if {path.stem for path in manifests.glob("*.json")} != {row["key"] for row in supported}:
        raise ValueError("Native manifests do not exactly cover the supported source inventory")
    cases = []
    for row in supported:
        if results.get(row["key"]) != "passed":
            raise ValueError(f"Package campaign did not pass: {row['key']}")
        manifest = manifests / (row["key"] + ".json")
        camera = json.loads(manifest.read_text())
        if camera["source"] != row["source"] or camera["level"] != row["level"] or len(camera["views"]) != 1:
            raise ValueError(f"Wrong source/world/camera manifest: {row['key']}")
        if digest(root / row["source"]) != row["source_sha256"]:
            raise ValueError(f"Source changed since package inventory: {row['key']}")
        build = next(command for command in commands[row["key"]] if command[1] == "build")
        package = Path(build[build.index("--out") + 1])
        if not package.is_file():
            raise ValueError(f"Package missing: {package}")
        cases.append(dict(key=row["key"], level=row["level"], source=row["source"],
                          manifest=str(manifest.resolve()), package=str(package.resolve()),
                          source_sha256=row["source_sha256"], package_sha256=digest(package)))
    return cases


def check_geometry(source: str, level: str, report: dict, exit_code: int) -> None:
    if report.get("format") != "places-geometry-check" or report["level"]["id"] != level:
        raise ValueError("Wrong geometry report/world")
    invalid = source == "tests/fixtures/levels/invalid/geometry_invalid.json"
    if report["level"]["validated"] != (not invalid):
        raise ValueError("Unexpected loader validation result")
    errors = {finding["check"] for finding in report["findings"] if finding["severity"] == "Error"}
    warnings = {finding["check"] for finding in report["findings"] if finding["severity"] == "Warning"}
    if errors != EXPECTED_ERRORS.get(source, set()):
        raise ValueError(f"Unexpected confirmed geometry checks: {sorted(errors)}")
    if not EXPECTED_WARNINGS.get(source, set()).issubset(warnings):
        raise ValueError("A named planted/review warning disappeared")
    if exit_code != (1 if errors else 0):
        raise ValueError("Checker process status differs from its report")
    if report["summary"]["errors"] != sum(finding["severity"] == "Error" for finding in report["findings"]):
        raise ValueError("Geometry summary differs from findings")
    if invalid:
        messages = [finding["message"] for finding in report["findings"] if finding["check"] == "curved-invalid"]
        if not all(any(word in message for message in messages) for word in ("thinner", "sweep", "radius")):
            raise ValueError("Degenerate curve contract was not named")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--campaign", type=Path, required=True, help="Completed regression_maps package campaign")
    parser.add_argument("--manifests", type=Path, default=ROOT / "tests/fixtures/native/map-manifests")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--out", type=Path, required=True, help="New immutable evidence directory")
    parser.add_argument("--geometry", action="store_true", help="CPU checker only; no window/capture")
    args = parser.parse_args()
    if args.out.exists():
        parser.error("Preserve evidence: --out must not exist")
    cases = plan(args.campaign, args.manifests)
    if args.geometry:
        inventory = json.loads((args.campaign / "inventory.json").read_text())["maps"]
        cases.extend(dict(key=row["key"], level=row["level"], source=row["source"],
                          source_sha256=row["source_sha256"]) for row in inventory
                     if row["classification"] == "loader-negative")
    binary = args.binary.resolve()
    frozen = {str(path): digest(path) for path in (binary, ROOT / "assets/catalog.json",
              ROOT / "tools/bench/capture_art_style_hero.py")}
    write_new(args.out / "plan.json", dict(phase="geometry" if args.geometry else "normal-high-native",
                                          frozen=frozen, cases=cases,
                                          offline_contract="capacity_beyond_former_limits remains a positive CPU budget/u32 witness, covered by the Rust gate; not a playable native scene."))
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    results = []
    for case in cases:
        output = args.out / case["key"]
        output.mkdir()
        if args.geometry:
            command = [str(binary), "--check-geometry", "--level", str(ROOT / case["source"]),
                       "--json", str((output / "geometry.json").resolve())]
            env["PLACES_ASSET_ROOT"] = str(ROOT)
        else:
            command = [sys.executable, str(ROOT / "tools/bench/capture_art_style_hero.py"),
                       "--manifest", case["manifest"], "--package", case["package"],
                       "--binary", str(binary), "--asset-root", str(ROOT), "--quality", "high",
                       "--out", str((output / "capture").resolve())]
        started = time.monotonic()
        record = dict(key=case["key"], command=command, started_utc=datetime.now(timezone.utc).isoformat())
        try:
            with (output / "command.log").open("x") as stream:
                completed = subprocess.run(command, cwd=ROOT, env=env, stdout=stream,
                                           stderr=subprocess.STDOUT, check=False)
            record["exit_code"] = completed.returncode
            if args.geometry:
                check_geometry(case["source"], case["level"],
                               json.loads((output / "geometry.json").read_text()), completed.returncode)
            elif completed.returncode != 0:
                raise ValueError(f"Native capture command exited {completed.returncode}")
            else:
                capture = json.loads((output / "capture/manifest.json").read_text())
                if len(capture) != 1 or capture[0]["identity"]["package_sha256"] != case["package_sha256"]:
                    raise ValueError("Capture did not use the planned frozen package")
                log = (output / "capture" / (capture[0]["view"]["name"] + ".log")).read_text()
                if "BENCH_SUMMARY" not in log or any(message in log for message in (
                        "panicked", "[wgpu] fatal device error", "Validation Error", "invalid surface")):
                    raise ValueError("Native renderer reported a device/validation/lifecycle failure")
                if digest(Path(case["package"])) != case["package_sha256"]:
                    raise ValueError("Package changed during native case")
            if digest(ROOT / case["source"]) != case["source_sha256"]:
                raise ValueError("Source changed during case")
            record["status"] = "passed"
        except (ValueError, OSError) as error:
            record.update(status="failed", error=str(error))
        record["elapsed_seconds"] = time.monotonic() - started
        results.append(record)
        write_new(output / "result.json", record)
        print(f"{record['status']}: {case['key']}", flush=True)
        if any(digest(Path(path)) != expected for path, expected in frozen.items()):
            results.append(dict(status="failed", error="Frozen native/catalogue/capture tool identity changed"))
            break
    failures = [record for record in results if record["status"] == "failed"]
    write_new(args.out / "results.json", dict(cases=results, failures=failures,
              completed=len(results) == len(cases), scope="Genuine normal High captures or separate CPU findings; no FPS or visual review claim."))
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())

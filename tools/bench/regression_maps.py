#!/usr/bin/env python3
"""Inventory every maintained and local map; run the normal package regression path.

Inventory is read-only unless --out is supplied. --run-packages is an explicit,
serialized campaign using an already built compiler; it never invokes cargo,
changes sources, or reduces the normal off,medium,full variants. Known negative
fixtures stay in the inventory with their real intended contracts.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
MAP_ROOTS = ("assets/levels", "levels", "tests/fixtures/levels")
PROGRESSIVE_SOURCES = (
    "tests/fixtures/levels/home_showcase.json", "tests/fixtures/levels/office_asset_review.json",
    "tests/fixtures/levels/pool_showcase.json", "tests/fixtures/levels/outdoor_kit_showcase.json",
    "assets/levels/winter.json", "assets/levels/lantern_hollow.json",
)
SPECIAL = {
    "tests/fixtures/levels/invalid/geometry_invalid.json": (
        "loader-negative", "Degenerate curves must be rejected; geometry_check::tests retains curved-invalid controls."),
    "tests/fixtures/levels/capacity_beyond_former_limits.json": (
        "cpu-boundary-witness", "Positive loader/count/u32-material-index witness with synthetic cap:* IDs; not catalogue-authored playable content. See tools/levels/build_capacity_fixtures.py and src/zoo_audit.rs."),
    "tests/fixtures/levels/geometry_broken.json": (
        "playable-geometry-negative", "Loader-valid planted defects; build/decode/native remain covered and checker defects must remain named."),
    "tests/fixtures/levels/repair/wall_step_x.json": (
        "playable-geometry-negative", "Loader-valid repair control; wall-joint-step must remain a confirmed checker error."),
    "tests/fixtures/levels/repair/wall_step_z.json": (
        "playable-geometry-negative", "Loader-valid repair control; wall-joint-step must remain a confirmed checker error."),
    "tests/fixtures/levels/repair/gap_review.json": (
        "playable-review-control", "Authored gap requires review rather than automatic alignment repair; preserve checker output."),
}


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def inventory(root: Path = ROOT) -> list[dict]:
    rows = []
    for directory in MAP_ROOTS:
        base = root / directory
        for source in sorted(base.rglob("*.json")):
            relative = source.relative_to(root).as_posix()
            level = json.loads(source.read_text())
            classification, reason = SPECIAL.get(relative, ("playable", "Normal catalogue-authored source."))
            sibling = source.with_suffix(".placesmap")
            rows.append(dict(key=relative.removesuffix(".json").replace("/", "__"),
                             source=relative, source_sha256=digest(source), level=level["id"],
                             classification=classification, scope_reason=reason,
                             package=sibling.relative_to(root).as_posix() if sibling.is_file() else None,
                             package_sha256=digest(sibling) if sibling.is_file() else None,
                             spawn=level.get("spawn", {}), props=len(level.get("props", [])),
                             spawn_templates=len(level.get("spawn_templates", [])),
                             authored_environment=level.get("environment"),
                             status="planned"))
        for package in sorted(base.rglob("*.placesmap")):
            if package.with_suffix(".json").is_file():
                continue
            with zipfile.ZipFile(package) as archive:
                manifest = json.loads(archive.read("manifest.json"))
            relative = package.relative_to(root).as_posix()
            rows.append(dict(key=relative.removesuffix(".placesmap").replace("/", "__"),
                             source=None, level=manifest["id"], package=relative,
                             package_sha256=digest(package), classification="package-only",
                             scope_reason="Preserve/validate installed legacy package; recover authoring source before required-current/full rebuild. No invented source currency.",
                             variants=[variant["lightmap_quality"] for variant in manifest["variants"]],
                             status="planned"))
    order = {source: index for index, source in enumerate(PROGRESSIVE_SOURCES)}
    return sorted(rows, key=lambda row: (order.get(row["source"], len(order)),
                                        row["source"] or row["package"]))


def supported(row: dict) -> bool:
    return row["classification"] not in ("loader-negative", "cpu-boundary-witness", "package-only")


def planned_commands(row: dict, compiler: Path, out: Path, root: Path = ROOT,
                     prepared_root: Path | None = None,
                     install_source_packages: bool = False) -> list[list[str]]:
    """Keep each source path distinct even when authored level IDs coincide."""
    if row["classification"] == "cpu-boundary-witness":
        return []
    if row["source"] is None:
        return [[str(compiler), "validate", str(root / row["package"]), "--json"]]
    source = root / row["source"]
    installed = row["source"].startswith(("assets/levels/", "levels/"))
    package = source.with_suffix(".placesmap") if installed and install_source_packages else (
        prepared_root or out / "packages") / (row["key"] + ".placesmap")
    build = [str(compiler), "build", str(source), "--out", str(package),
             "--asset-root", str(root / "assets"), "--workers", "12", "--json"]
    if row["classification"] == "loader-negative":
        return [build]
    return [build,
            [str(compiler), "verify", str(source), "--package", str(package),
             "--asset-root", str(root / "assets"), "--require-current", "--json"],
            [str(compiler), "validate", str(package), "--json"]]


def write_new(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as stream:
        json.dump(value, stream, indent=2)
        stream.write("\n")


def run_packages(rows: list[dict], compiler: Path, out: Path,
                 prepared_root: Path | None = None,
                 install_source_packages: bool = False) -> int:
    """Record every result and retain independent failures without changing inputs."""
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    compiler_sha = digest(compiler)
    results = []
    failures = []
    for row in rows:
        commands = planned_commands(row, compiler, out, prepared_root=prepared_root,
                                    install_source_packages=install_source_packages)
        if not commands:
            results.append(dict(key=row["key"], status="intended-cpu-contract", reason=row["scope_reason"]))
            continue
        records = []
        for number, command in enumerate(commands):
            if command[1] == "build":
                Path(command[command.index("--out") + 1]).parent.mkdir(parents=True, exist_ok=True)
            start_utc = datetime.now(timezone.utc).isoformat()
            start = time.monotonic()
            log_path = out / "commands" / f"{row['key']}-{number}.log"
            log_path.parent.mkdir(parents=True, exist_ok=True)
            try:
                with log_path.open("x") as stream:
                    result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream,
                                            stderr=subprocess.STDOUT, timeout=3600, check=False)
                code = result.returncode
            except subprocess.TimeoutExpired:
                code = None
            records.append(dict(command=command, started_utc=start_utc, exit_code=code,
                                elapsed_seconds=time.monotonic() - start, log=str(log_path)))
            if row["classification"] == "loader-negative":
                rejected_for_contract = (code == 1 and
                    "Arc wall 0 thickness must be positive and thinner than twice its radius" in log_path.read_text()
                    and not Path(command[command.index("--out") + 1]).exists())
                status = "expected-named-loader-rejection" if rejected_for_contract else "failed"
                if status == "failed":
                    failures.append(row["key"])
            elif code != 0:
                failures.append(row["key"])
                status = "failed"
                break
            else:
                status = "passed"
        results.append(dict(key=row["key"], status=status, commands=records))
        write_new(out / "commands" / f"{row['key']}.json", results[-1])
        print(f"{status}: {row['key']}", flush=True)
    stable = digest(compiler) == compiler_sha
    if not stable:
        failures.append("compiler-changed-during-campaign")
    write_new(out / "package-results.json", dict(compiler_sha256=compiler_sha,
                                               compiler_stable=stable, cases=results,
                                               failures=failures))
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="New campaign directory; required for --run-packages")
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/release/places-compile")
    parser.add_argument("--prepared-root", type=Path,
                        help="Stable fixture package directory for safe unchanged reuse; receipts remain under new --out")
    parser.add_argument("--install-source-packages", action="store_true",
                        help="Explicitly build shipped/local source siblings; requires preserved entry packages and content freeze")
    parser.add_argument("--run-packages", action="store_true")
    args = parser.parse_args()
    if args.run_packages and not args.out:
        parser.error("--run-packages requires a new --out directory")
    if args.out and args.out.exists():
        parser.error("Preserve existing evidence; --out must not exist")
    rows = inventory()
    prepared_root = args.prepared_root.resolve() if args.prepared_root else None
    report = dict(format_version=1, map_roots=MAP_ROOTS,
                  inventory_utc=datetime.now(timezone.utc).isoformat(),
                  install_source_packages=args.install_source_packages,
                  prepared_root=str(prepared_root) if prepared_root else None,
                  sources=sum(row["source"] is not None for row in rows),
                  supported_sources=sum(supported(row) for row in rows), maps=rows)
    if args.out:
        write_new(args.out / "inventory.json", report)
        write_new(args.out / "plan.json", [dict(key=row["key"], commands=planned_commands(
            row, args.compiler.resolve(), args.out.resolve(), prepared_root=prepared_root,
            install_source_packages=args.install_source_packages)) for row in rows])
    else:
        print(json.dumps(report, indent=2))
    return run_packages(rows, args.compiler.resolve(), args.out.resolve(), prepared_root,
                        args.install_source_packages) if args.run_packages else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Benchmark and validate safe package reuse with preserved, distinct source edits.

Each sample starts from a fresh baseline package, applies one edit, then compares
its incremental package against a forced clean build of precisely the edited
source. Timings exclude the setup and independent correctness rebuild. All
sources and playable packages remain in the selected durable output collection.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import statistics
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from compiler_bench import parse_log, run_owned, save_report
from compare_packages import compare


def edited_source(original, case):
    source = copy.deepcopy(original)
    if case == "metadata":
        source["name"] += " — compiler metadata regression"
        source["author"] = "Compiler regression"
    elif case == "light":
        source["ceiling_lights"][0]["x"] += 0.125
    elif case == "prop":
        source["props"][0]["x"] += 0.125
    elif case == "material":
        source["defaults"]["floor"] = "core:carpet_damp_01"
    elif case != "unchanged":
        raise ValueError(f"unknown case: {case}")
    return source


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, default=ROOT / "tests/fixtures/levels/test_room.json")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--workers", type=int, default=12)
    parser.add_argument("--variants", default="off,medium,full")
    parser.add_argument("--cases", nargs="+", default=["unchanged", "metadata", "light", "prop", "material"])
    parser.add_argument("--expect-metadata-reuse", action="store_true")
    parser.add_argument("--seed-package", type=Path,
                        help="verified existing clean package; setup copy is excluded from timings")
    parser.add_argument("--reference-once", action="store_true",
                        help="compare repeated identical edits with one independent clean reference")
    args = parser.parse_args()
    if args.repeat < 1 or args.workers not in range(1, 13):
        parser.error("positive repeats and 1..12 workers required")
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    original_bytes = args.source.read_bytes()
    original = json.loads(original_bytes)
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    env.update(PLACES_VERBOSE="1", PLACES_STATE_ROOT=str(args.out / "state"))
    report = dict(binary=str(args.binary.resolve()), source=str(args.source.resolve()),
                  binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                  source_sha256=hashlib.sha256(original_bytes).hexdigest(),
                  variants=args.variants, workers=args.workers, runs=[], medians=[])
    report["reference_policy"] = "one clean reference per exact edited input" if args.reference_once else "clean rebuild per sample"
    references = {}
    if args.seed_package:
        args.seed_package = args.seed_package.resolve()
        with zipfile.ZipFile(args.seed_package) as archive:
            manifest = json.loads(archive.read("manifest.json"))
        if {variant["lightmap_quality"] for variant in manifest["variants"]} != set(args.variants.split(",")):
            parser.error("seed package must contain exactly the requested variants")
        verification = run_owned([str(args.binary.resolve()), "verify", str(args.source.resolve()),
                                  "--package", str(args.seed_package), "--asset-root", str(ROOT / "assets"),
                                  "--require-current", "--json"], args.out / "seed-verify.log", env, 3600)
        if verification["exit_code"]:
            raise RuntimeError("seed package is corrupt or stale for this exact source/compiler")
        report["seed_package"] = str(args.seed_package)
        report["seed_sha256"] = hashlib.sha256(args.seed_package.read_bytes()).hexdigest()

    def build(source, package, label, force):
        command = [str(args.binary.resolve()), "build", str(source), "--out", str(package),
                   "--asset-root", str(ROOT / "assets"), "--workers", str(args.workers),
                   "--variants", args.variants, "--json"]
        if force:
            command.append("--force")
        log = args.out / f"{label}.log"
        result = run_owned(command, log, env, 3600)
        if result["exit_code"]:
            raise RuntimeError(f"build failed: {log}")
        result["build"], result["phases"], result["scenes"] = parse_log(log.read_text())
        result["log"] = str(log)
        result["package"] = str(package)
        return result

    for case in args.cases:
        for repeat in range(1, args.repeat + 1):
            label = f"{case}-r{repeat}"
            print(f"Starting incremental {label}", flush=True)
            source = args.out / f"{label}.json"
            package = args.out / f"{label}.placesmap"
            source.write_bytes(original_bytes)
            if args.seed_package:
                shutil.copy2(args.seed_package, package)
                setup = dict(wall_seconds=0.0)
            else:
                setup = build(source, package, f"{label}-setup", True)
            shutil.copy2(source, args.out / f"{label}-before.json")
            shutil.copy2(package, args.out / f"{label}-before.placesmap")
            if case != "unchanged":
                source.write_text(json.dumps(edited_source(original, case), indent=2) + "\n")
            result = build(source, package, label, False)
            source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
            reused_reference = args.reference_once and source_hash in references
            if reused_reference:
                fresh = references[source_hash]
            elif args.reference_once and case == "unchanged" and args.seed_package:
                # The verified seed is the recorded independent clean build of
                # precisely these unchanged bytes and requested variants.
                fresh = args.seed_package
            else:
                fresh = args.out / f"{label}-clean.placesmap"
                build(source, fresh, f"{label}-clean", True)
            references[source_hash] = fresh
            equality = compare(package, fresh)
            if not equality["archive_byte_equal"]:
                raise RuntimeError(f"incremental output differs from clean: {label}: {equality}")
            reused = any("reused prepared geometry" in warning for warning in result["build"]["warnings"])
            expected = case == "metadata" and args.expect_metadata_reuse
            if reused != expected or (case == "unchanged") == result["build"]["rebuilt"]:
                raise RuntimeError(f"unexpected reuse decision: {label}: {result['build']}")
            result.update(case=case, repeat=repeat, setup_wall_seconds=setup["wall_seconds"],
                          exact_clean_equality=True, reused_static_stage=reused,
                          source_sha256=source_hash, clean_reference=str(fresh),
                          clean_reference_sha256=hashlib.sha256(fresh.read_bytes()).hexdigest(),
                          repeated_clean_reference=reused_reference)
            report["runs"].append(result)
            save_report(args.out / "report.json", report)
            print(f"Finished {label}: {result['wall_seconds']:.3f}s; exact clean equality", flush=True)
        runs = [run for run in report["runs"] if run["case"] == case]
        report["medians"].append(dict(case=case, count=len(runs), **{
            field: statistics.median(run[field] for run in runs)
            for field in ("wall_seconds", "peak_rss_bytes", "cpu_seconds", "effective_cores")}))
        save_report(args.out / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

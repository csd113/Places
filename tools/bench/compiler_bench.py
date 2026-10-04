#!/usr/bin/env python3
"""Measure complete, unchanged-quality map builds in a native desktop session.

Runs are sequential: each compiler owns the entire requested worker allocation.
Every clean sample uses --force, a separate package, and a fresh process. Reused
packages are measured separately with --reuse. Raw logs/packages accompany the
atomic JSON report so timings and output comparisons remain independently usable.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import sys
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def native_package_identity(package):
    """Runtime keys hash the canonical compiler-authored manifest, not ZIP bytes.

    These harness inputs are validated compiler outputs; their stored manifests
    are exactly Manifest::to_json bytes, including the final newline.
    """
    with zipfile.ZipFile(package) as archive:
        return hashlib.sha256(archive.read("manifest.json")).hexdigest()


def parse_log(text):
    """Read the CLI's final JSON and retain every independently timed solve."""
    start = text.rfind("\n{")
    report = json.loads(text[start + 1:] if start >= 0 else text)
    stages = []
    for line in text.splitlines():
        if "-timing]" in line:
            fields = dict(re.findall(r"([a-z_]+)=([^ ]+)", line))
            stages.append(dict(tag=line.split("]", 1)[0].lstrip("["), **fields))
    scenes = [dict(triangles=int(a), skipped=int(b), emitters=int(c), switchable=int(d))
              for a, b, c, d in re.findall(
                  r"transport scene triangles=(\d+) \((\d+) skipped\) emitters=(\d+) \((\d+) switchable\)", text)]
    for scene, work in zip(scenes, re.findall(r"\[lightmap-work\] ([^\n]+)", text)):
        scene["work"] = {key: int(value) for key, value in
                         re.findall(r"([a-z_]+)=(\d+)", work)}
    return report, stages, scenes


def run_owned(command, log, env, timeout, sample_seconds=0):
    """wait4 measures this child's peak RSS and CPU, including its worker threads."""
    started = time.perf_counter()
    started_utc = datetime.now(timezone.utc).isoformat()
    usage = None
    host_load = []
    with log.open("wb") as output:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        sampler = None
        next_load_sample = started
        try:
            if sample_seconds and sys.platform == "darwin":
                sampler = subprocess.Popen(
                    ["/usr/bin/sample", str(process.pid), str(sample_seconds), "2",
                     "-file", str(log.with_suffix(".sample.txt"))],
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if hasattr(os, "wait4"):
                while True:
                    pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                    if pid:
                        process.returncode = os.waitstatus_to_exitcode(status)
                        break
                    if time.perf_counter() - started > timeout:
                        raise subprocess.TimeoutExpired(command, timeout)
                    if time.perf_counter() >= next_load_sample:
                        try:
                            host_load.append(dict(elapsed_seconds=time.perf_counter() - started,
                                                  load_average=os.getloadavg()))
                        except (AttributeError, OSError):
                            pass
                        next_load_sample = time.perf_counter() + 2
                    time.sleep(0.1)
            else:
                process.wait(timeout=timeout)
        finally:
            if process.returncode is None:
                if hasattr(os, "killpg"):
                    os.killpg(process.pid, signal.SIGTERM)
                else:
                    process.terminate()
                process.wait(timeout=10)
            if sampler is not None:
                sampler.wait(timeout=sample_seconds + 30)
    wall = time.perf_counter() - started
    cpu = usage.ru_utime + usage.ru_stime if usage is not None else None
    # Darwin returns bytes; Linux returns KiB. Keep the native value as evidence.
    rss = usage.ru_maxrss if usage is not None else None
    rss_bytes = rss if sys.platform == "darwin" else rss * 1024 if rss is not None else None
    return dict(exit_code=process.returncode, pid=process.pid,
                started_utc=started_utc, finished_utc=datetime.now(timezone.utc).isoformat(),
                host_load_samples=host_load, wall_seconds=wall, cpu_seconds=cpu,
                effective_cores=cpu / wall if cpu is not None else None,
                peak_rss_bytes=rss_bytes, raw_maxrss=rss,
                voluntary_switches=usage.ru_nvcsw if usage is not None else None,
                involuntary_switches=usage.ru_nivcsw if usage is not None else None)


def save_report(path, report):
    temporary = path.with_suffix(".partial")
    temporary.write_text(json.dumps(report, indent=2) + "\n")
    temporary.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places-compile")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--maps", nargs="+", default=["tests/fixtures/levels/test_room.json",
                        "assets/levels/places_demo.json", "assets/levels/lantern_hollow.json"])
    parser.add_argument("--workers", nargs="+", type=int, default=[12])
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--variants", default="off,medium,full")
    parser.add_argument("--reuse", action="store_true")
    parser.add_argument("--baseline", type=Path,
                        help="require exact payload equality with this baseline package directory")
    parser.add_argument("--sample-seconds", type=int, default=0,
                        help="separate diagnostic runs only; sampling perturbs timings")
    parser.add_argument("--timeout", type=float, default=3600)
    parser.add_argument("--label", default="", help="record resource-coordination conditions")
    args = parser.parse_args()
    if args.repeat < 1 or args.timeout <= 0 or any(count < 1 or count > 12 for count in args.workers):
        parser.error("repeat/timeout must be positive; worker allocations must be in 1..12")
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    report = dict(binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                  platform=sys.platform, available_cpus=os.cpu_count(), variants=args.variants,
                  mode="package-reuse" if args.reuse else "clean-force", runs=[], medians=[])
    report["label"] = args.label
    if sys.platform == "darwin":
        result = subprocess.run(["/usr/sbin/sysctl", "-n", "hw.model", "hw.ncpu", "hw.memsize"],
                                capture_output=True, text=True, check=False)
        report["hardware"] = result.stdout.splitlines()
    save_report(args.out / "report.json", report)
    env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
    env["PLACES_VERBOSE"] = "1"
    env["PLACES_STATE_ROOT"] = str(args.out / "state")
    for source in args.maps:
        source = (ROOT / source).resolve()
        for workers in args.workers:
            for index in range(args.repeat):
                name = f"{source.stem}-w{workers}-r{index + 1}"
                package = args.out / f"{name}.placesmap"
                if args.reuse:
                    package = args.out / f"{source.stem}-w{workers}-r1.placesmap"
                log = args.out / f"{name}.log"
                command = [str(binary), "build", str(source), "--out", str(package),
                           "--asset-root", str(ROOT / "assets"), "--workers", str(workers),
                           "--variants", args.variants, "--json"]
                if not args.reuse:
                    command.append("--force")
                print(f"Starting {name}", flush=True)
                run = dict(map=str(source), workers=workers, repeat=index + 1,
                           cold_process=True, first_map_sample=index == 0,
                           package=str(package), log=str(log), command=command,
                           source_sha256=hashlib.sha256(source.read_bytes()).hexdigest())
                run.update(run_owned(command, log, env, args.timeout, args.sample_seconds))
                if run["exit_code"] == 0:
                    run["build"], run["phases"], run["scenes"] = parse_log(log.read_text())
                    run["package_sha256"] = hashlib.sha256(package.read_bytes()).hexdigest()
                    if args.baseline:
                        from compare_packages import compare
                        baseline = args.baseline / f"{source.stem}-w12-r2.placesmap"
                        if not baseline.is_file():
                            baseline = args.baseline / f"{source.stem}-w12-r1.placesmap"
                        equality = compare(baseline, package, asset_root=ROOT / "assets")
                        save_report(args.out / f"{name}-quality.json", equality)
                        run["quality_equal"] = equality["quality_equal"]
                        run["baseline_package"] = str(baseline.resolve())

                report["runs"].append(run)
                save_report(args.out / "report.json", report)
                print(f"Finished {name}: {run['wall_seconds']:.3f}s, exit {run['exit_code']}", flush=True)
                if run["exit_code"]:
                    return run["exit_code"]
                if run.get("quality_equal") is False:
                    print(f"Rejected output difference: {name}; retained comparison evidence", flush=True)
                    return 1
            runs = [run for run in report["runs"] if run["map"] == str(source) and run["workers"] == workers]
            summary = dict(map=str(source), workers=workers, count=len(runs))
            for field in ("wall_seconds", "cpu_seconds", "effective_cores", "peak_rss_bytes"):
                values = [run[field] for run in runs if run[field] is not None]
                summary[field] = statistics.median(values) if values else None
            report["medians"].append(summary)
            save_report(args.out / "report.json", report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

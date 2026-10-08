#!/usr/bin/env python3
"""Inspect validated package storage and native resource counters against warning budgets.

This read-only audit never changes renderer safety limits or hides content. Native
counts describe submitted ranges/objects, including depth-occluded geometry; they
do not establish fragment overdraw, presented FPS or total driver allocation.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import zipfile


def warning_budgets(metrics, limits):
    if any(key not in metrics or not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value <= 0
           for key, value in limits.items()):
        raise ValueError("Warning budgets require known metric names and positive numeric limits")
    return [dict(metric=key, observed=metrics[key], warning_budget=value)
            for key, value in limits.items() if metrics[key] > value]


def inspect(package, log, variant="full"):
    with zipfile.ZipFile(package) as archive:
        # Mirror the established package aggregate/count/manifest safety bounds.
        entries = archive.infolist()
        if (len(entries) > 512 or sum(item.file_size for item in entries) > 1024 ** 3
                or archive.getinfo("manifest.json").file_size > 2 * 1024 ** 2
                or any(item.file_size > 256 * 1024 ** 2 + 64 * 1024 for item in entries)
                or len({item.filename for item in entries}) != len(entries)):
            raise ValueError("Package exceeds reader safety bounds or has duplicate entries")
        manifest = json.loads(archive.read("manifest.json"))
        members = {item["name"]: item for item in manifest["entries"]}
        if len(members) != len(manifest["entries"]) or set(archive.namelist()) != set(members) | {"manifest.json"}:
            raise ValueError("Archive entries do not match the declared manifest")
        for name, entry in members.items():
            data = archive.read(name)
            if len(data) != entry["bytes"] or hashlib.sha256(data).hexdigest() != entry["sha256"]:
                raise ValueError("Unverified package entry: " + name)
        active = next(item for item in manifest["variants"] if item["lightmap_quality"] == variant)
        roles = {}
        for name, entry in members.items():
            totals = roles.setdefault(entry["role"], dict(stored_bytes=0, compressed_bytes=0))
            totals["stored_bytes"] += entry["bytes"]
            totals["compressed_bytes"] += archive.getinfo(name).compress_size
        atlas = active["entries"].get("lightmaps_meta")
        metadata = json.loads(archive.read(atlas)) if atlas else None
    text = log.read_text()
    summaries = re.findall(r"^BENCH_SUMMARY (.*)$", text, re.MULTILINE)
    if not summaries:
        raise ValueError("Native benchmark summary required")
    native = json.loads(summaries[-1])
    captures = []
    for line in text.splitlines():
        if line.startswith("[visual-diagnostic] "):
            receipt = json.loads(line.removeprefix("[visual-diagnostic] "))
            if receipt.get("event") == "capture" and receipt.get("mode") == "final":
                captures.append(receipt)
    capture = captures[-1] if captures else None
    submission = capture.get("capture_submission") if capture else None
    if native["level"] != manifest["id"] or (capture and capture["level"] != manifest["id"]):
        raise ValueError("Native level differs from the inspected package")
    package_ids = re.findall(r"package identity level=\S+ variant=(\S+) sha256=(\w+)", text)
    with zipfile.ZipFile(package) as archive:
        expected_identity = hashlib.sha256(archive.read("manifest.json")).hexdigest()
    if not package_ids or package_ids[-1] != (variant, expected_identity):
        raise ValueError("Native package identity or quality does not match")
    if submission:
        if (submission["draw_calls"] <= 0 or submission["frustum_visible_distinct_vertices"] <= 0
                or capture["resident"]["lightmaps"] != variant):
            raise ValueError("Reject empty or incompatible capture submission")
        metrics = dict(draw_calls=submission["draw_calls"],
                       visible_vertices=submission["frustum_visible_distinct_vertices"],
                       material_changes=submission["material_binds"],
                       submitted_indices=submission["submitted_indices"],
                       submitted_triangles=submission["submitted_triangles"])
        counter_source = "actual final native capture submission; surface telemetry may be zero"
    else:
        if native["draw_calls"] <= 0 or native["visible_vertices"] <= 0:
            raise ValueError("Reject zero-draw native telemetry for a populated scene budget")
        metrics = {key: native[key] for key in ["total_vertices", "visible_vertices", "draw_calls",
                                               "vbo_bytes", "index_bytes", "material_changes"]}
        if "visible_indices" in native:
            metrics.update(submitted_indices=native["visible_indices"],
                           submitted_triangles=native["visible_indices"] // 3)
        counter_source = "nonzero surface telemetry; timing/FPS is outside this audit"
    metrics["package_bytes"] = package.stat().st_size
    matches = re.findall(r"textures: .*?\((\d+) bytes resident", text)
    if matches:
        metrics["world_texture_resident_bytes"] = int(matches[-1])
    props = re.findall(r"\[props\] (\d+) models cached .*?, (\d+) triangles, (\d+) KiB of textures", text)
    if props:
        metrics.update(cached_models=int(props[-1][0]), source_model_triangles=int(props[-1][1]),
                       source_prop_decoded_bytes=int(props[-1][2]) * 1024)
    if metadata:
        metrics.update(lightmap_pages=metadata["page_count"], lightmap_charts=len(metadata["charts"]),
                       lightmap_resident_bytes=metadata["page_count"] * metadata["page_edge"] ** 2
                       * 16 * (1 + len(metadata["switchable_lights"])))
    return dict(package=str(package.resolve()), package_sha256=hashlib.sha256(package.read_bytes()).hexdigest(),
                native_log=str(log.resolve()), variant=variant, metrics=metrics, stored_roles=roles,
                counter_source=counter_source, capture_resident=capture.get("resident") if capture else None,
                scope="Base scene submission; sky/emission/post/UI excluded from draws. Allocations are separate inventories, not total GPU memory. Transparency fragment overdraw is not measured.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", required=True, type=Path)
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("--variant", default="full", choices=("off", "medium", "full"))
    parser.add_argument("--limits", type=Path, help="Optional JSON metric-to-warning-budget mapping")
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("Preserve the earlier audit; select a new output path")
    report = inspect(args.package, args.log, args.variant)
    limits = json.loads(args.limits.read_text()) if args.limits else {}
    report.update(warning_budgets=limits, warnings=warning_budgets(report["metrics"], limits))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(dict(out=str(args.out), warnings=report["warnings"])))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

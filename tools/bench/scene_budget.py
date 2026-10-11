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

MAX_ATLAS_RECORD_BYTES = 320 * 1024 ** 2 + 64 * 1024


def declared_entry_limit(entry):
    """Mirror compiler.rs::declared_entry_limit; typed records tighten below."""
    if entry["name"] == "semantics.json":
        return 64 * 1024 ** 2
    if entry["role"] == "lightmaps":
        return MAX_ATLAS_RECORD_BYTES
    if entry["role"] in {"mesh", "props"}:
        return 512 * 1024 ** 2
    return 256 * 1024 ** 2


def validate_archive_bounds(archive):
    """Check declared sizes before payload reads using existing runtime bounds.

    package/mod.rs supplies the constants; compiler.rs and package/world.rs
    select them for integrity reads and typed variant records. Payload decoding
    remains the ordinary compiler validator's responsibility.
    """
    entries = archive.infolist()
    if (len(entries) > 512 or sum(item.file_size for item in entries) > 1024 ** 3
            or archive.getinfo("manifest.json").file_size > 2 * 1024 ** 2
            or len({item.filename for item in entries}) != len(entries)):
        raise ValueError("Package exceeds reader safety bounds or has duplicate entries")
    manifest = json.loads(archive.read("manifest.json"))
    members = {item["name"]: item for item in manifest["entries"]}
    if len(members) != len(manifest["entries"]) or set(archive.namelist()) != set(members) | {"manifest.json"}:
        raise ValueError("Archive entries do not match the declared manifest")
    typed_limits = {"semantics.json": 64 * 1024 ** 2, "build-inputs.json": 2 * 1024 ** 2}
    variant_limits = {"mesh": 512 * 1024 ** 2, "props": 512 * 1024 ** 2,
                      "lighting": 256 * 1024 ** 2, "collision": 128 * 1024 ** 2,
                      "navigation": 128 * 1024 ** 2,
                      "lightmaps": MAX_ATLAS_RECORD_BYTES,
                      "lightmaps_meta": 64 * 1024 ** 2, "irradiance": 256 * 1024 ** 2}
    for variant in manifest["variants"]:
        records = variant["entries"]
        for key, limit in variant_limits.items():
            if records.get(key):
                name = records[key]
                typed_limits[name] = min(typed_limits.get(name, limit), limit)
        for probes in records.get("probes", []):
            typed_limits[probes["positions"]] = min(typed_limits.get(probes["positions"], 16 * 1024 ** 2), 16 * 1024 ** 2)
            for name in probes["cubemaps"]:
                typed_limits[name] = min(typed_limits.get(name, 256 * 1024 ** 2), 256 * 1024 ** 2)
    for name, entry in members.items():
        size = archive.getinfo(name).file_size
        limit = min(declared_entry_limit(entry), typed_limits.get(name, 512 * 1024 ** 2))
        if size != entry["bytes"]:
            raise ValueError("Declared package entry size differs: " + name)
        if size > limit:
            raise ValueError(f"Package entry '{name}' exceeds its {limit}-byte record safety bound")
    return manifest, members


def warning_budgets(metrics, limits):
    if any(key not in metrics or not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value) or value <= 0
           for key, value in limits.items()):
        raise ValueError("Warning budgets require known metric names and positive numeric limits")
    return [dict(metric=key, observed=metrics[key], warning_budget=value)
            for key, value in limits.items() if metrics[key] > value]


def inspect(package, log, variant="full"):
    with zipfile.ZipFile(package) as archive:
        manifest, members = validate_archive_bounds(archive)
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

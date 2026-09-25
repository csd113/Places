#!/usr/bin/env python3
"""Builds and validates the places core prop pack.

Usage (from the Places repository root)::

    python3 tools/props/build.py                # rebuild every prop + report
    python3 tools/props/build.py --only core:chair core:crate
    python3 tools/props/build.py --check        # validate the shipped GLBs only
    python3 tools/props/build.py --report       # print the budget table only

The catalogue ``assets/catalog.json`` is the authoritative scope: the
generator builds exactly the placeable entries it lists (props and entities, no
more, no fewer), writes each ``model`` path it declares (relative to
``assets/``), and refuses to ship a prop whose mesh breaks the pack's scale,
origin, UV or triangle budgets.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Dict, List

HERE = os.path.dirname(os.path.abspath(__file__))
APP_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASSET_ROOT = os.path.join(APP_ROOT, "assets")
CATALOG_PATH = os.path.join(ASSET_ROOT, "catalog.json")
PROXY_PATH = os.path.join(ASSET_ROOT, "prop_proxies.json")
THUMB_DIR = os.path.join(APP_ROOT, "level-editor", "assets", "thumbs")

sys.path.insert(0, HERE)
sys.path.insert(0, os.path.dirname(HERE))

import glb  # noqa: E402
import parts  # noqa: E402
from mesh import PropBuilder  # noqa: E402

# The shipped art budgets. These are deliberately tighter than the engine's
# limits: the builder refuses to ship art over budget even though the runtime
# accepts more. The engine ceilings live in src/level.rs
# (PROP_TRIANGLE_BUDGET / PROP_TEXTURE_NATIVE_SIZE / MAX_PROP_TEXTURE_SIZE name
# the same art budgets: 1500 triangles and a 256 px native texture; the engine
# accepts 6000 triangles and 1024 px textures, downscaled at runtime). Mirrored
# by the Rust budget suite (src/props/tests.rs) and the editor tests.
TRIANGLE_TARGET = 500
TRIANGLE_REVIEW = 800
TRIANGLE_ART_MAX = 1500
TRIANGLE_ENGINE_MAX = 6000
# 256x256 is the normal native prop texture size. A GLB embeds its runtime
# atlas, and Full already samples prop sheets at 256, so shipping embedded art
# above the native size would spend GLB bytes on pixels no profile displays.
# Source/master artwork may still be kept larger beside the model for future
# quality work; the renderer itself accepts GLB images up to 1024 and
# downscales them to the active profile budget (Full 256, Low 128).
TEXTURE_NATIVE_MAX = 256
TEXTURE_ENGINE_MAX = 1024
# Decoded RGBA8 memory one shipped prop texture may embed: one engine-max
# image. Larger dimensions are rejected before they can allocate.
TEXTURE_DECODED_MAX = TEXTURE_ENGINE_MAX * TEXTURE_ENGINE_MAX * 4
# Whole-pack decoded RGBA8 budget. Deliberately desktop-scale: 64 MiB holds
# 256 native 256x256 sheets (the whole current pack is under 4 MiB), which
# leaves years of content growth before it binds, while still catching a
# pathological or accidentally duplicated multi-gigabyte set.
PACK_TEXTURE_MEMORY_MAX = 64 * 1024 * 1024

# Asset types the toolkit can build: entities place through the same pipeline.
PLACEABLE_TYPES = ("prop", "entity")


def load_catalog() -> dict:
    with open(CATALOG_PATH, "r", encoding="utf-8") as handle:
        return json.load(handle)


def catalog_placeables(catalog: dict) -> List[dict]:
    """The placeable (prop/entity) entries the toolkit builds and validates."""
    entries = catalog.get("assets")
    if entries is None:
        entries = catalog.get("props", [])
    return [
        entry
        for entry in entries
        if entry.get("asset_type", "prop") in PLACEABLE_TYPES
    ]


def entry_name(entry: dict) -> str:
    """The human-readable name, accepting the legacy ``name`` spelling."""
    return entry.get("display_name") or entry.get("name") or entry["id"]


def model_path(entry: dict) -> str:
    model = entry.get("model")
    if not model:
        raise SystemExit(f"{entry['id']}: catalogue entry has no model path")
    return os.path.join(ASSET_ROOT, model)


def build_one(entry: dict, build_fn) -> dict:
    prop_id = entry["id"]
    size = [float(value) for value in entry["size"]]
    builder = PropBuilder(prop_id, entry_name(entry), size)
    build_fn(builder)
    builder.mesh.validate(size)
    degenerate = builder.mesh.degenerate_triangles()
    if degenerate:
        raise SystemExit(f"{prop_id}: mesh contains {degenerate} degenerate (zero-area) triangles")

    triangles = builder.mesh.triangle_count
    if triangles > TRIANGLE_ART_MAX:
        raise SystemExit(
            f"{prop_id} has {triangles} triangles; the shipped art budget is {TRIANGLE_ART_MAX} "
            f"(target {TRIANGLE_TARGET}; the engine accepts up to {TRIANGLE_ENGINE_MAX})"
        )
    if builder.tex.width > TEXTURE_NATIVE_MAX or builder.tex.height > TEXTURE_NATIVE_MAX:
        raise SystemExit(
            f"{prop_id} texture is {builder.tex.width}x{builder.tex.height}; the shipped native "
            f"atlas is {TEXTURE_NATIVE_MAX}x{TEXTURE_NATIVE_MAX} "
            f"(the engine accepts up to {TEXTURE_ENGINE_MAX} and downscales at runtime)"
        )

    payload = glb.write_glb(builder.mesh, builder.tex.png_bytes(), name=prop_id.replace(":", "_"))
    destination = model_path(entry)
    os.makedirs(os.path.dirname(destination), exist_ok=True)
    with open(destination, "wb") as handle:
        handle.write(payload)

    low, high = builder.mesh.bounds()
    return {
        "id": prop_id,
        "name": entry_name(entry),
        "model": entry["model"],
        "triangles": triangles,
        "vertices": builder.mesh.vertex_count,
        "texture": [builder.tex.width, builder.tex.height],
        "decoded_texture_bytes": builder.tex.width * builder.tex.height * 4,
        "bytes": len(payload),
        "bounds_min": [round(value, 3) for value in low],
        "bounds_max": [round(value, 3) for value in high],
        "parts": builder.mesh.parts,
        "notes": builder.notes,
    }


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", nargs="*", default=None, help="build just these prop ids")
    parser.add_argument("--check", action="store_true", help="validate shipped GLBs without rebuilding")
    parser.add_argument("--report", action="store_true", help="print the budget report only")
    parser.add_argument("--no-proxies", action="store_true", help="skip prop_proxies.json for the editor")
    parser.add_argument("--thumbs", action="store_true", help="also render editor thumbnails (needs preview.py)")
    args = parser.parse_args(argv)

    catalog = load_catalog()
    entries = catalog_placeables(catalog)
    registry = parts.collect()

    if not args.check and not args.report:
        buildable = [
            entry
            for entry in entries
            if entry.get("model") and (not args.only or entry["id"] in args.only)
        ]
        missing = [entry["id"] for entry in buildable if entry["id"] not in registry]
        if missing:
            raise SystemExit(
                "no builder registered for: " + ", ".join(missing) + " (add it to tools/props/parts/*.py)"
            )
        extra = [prop_id for prop_id in registry if prop_id not in {entry["id"] for entry in entries}]
        if extra:
            print(f"warning: builders registered but not in the catalogue: {', '.join(sorted(extra))}")

    report: List[dict] = []
    failures: List[str] = []

    if args.check or args.report:
        for entry in entries:
            if not entry.get("model"):
                continue
            path = model_path(entry)
            if not os.path.exists(path):
                failures.append(f"{entry['id']}: missing model file {entry['model']}")
                continue
            with open(path, "rb") as handle:
                data = handle.read()
            try:
                mesh = glb.read_glb(data)
            except glb.GltfError as error:
                failures.append(f"{entry['id']}: {error}")
                continue
            low, high = mesh.bounds()
            pixels = None
            decoded = 0
            if mesh.texture_png.startswith(b"\x89PNG"):
                import struct as _struct

                width, height = _struct.unpack(">II", mesh.texture_png[16:24])
                pixels = [width, height]
                decoded = width * height * 4
                if width > TEXTURE_ENGINE_MAX or height > TEXTURE_ENGINE_MAX:
                    failures.append(
                        f"{entry['id']}: embedded texture is {width}x{height}, over the "
                        f"engine {TEXTURE_ENGINE_MAX}px limit"
                    )
                elif decoded > TEXTURE_DECODED_MAX:
                    failures.append(
                        f"{entry['id']}: embedded texture decodes to {decoded} bytes, over the "
                        f"{TEXTURE_DECODED_MAX}-byte per-texture ceiling"
                    )
                elif width > TEXTURE_NATIVE_MAX or height > TEXTURE_NATIVE_MAX:
                    print(
                        f"warning: {entry['id']}: embedded texture is {width}x{height}, above the "
                        f"{TEXTURE_NATIVE_MAX}px native size; Full will downsample it to "
                        f"{TEXTURE_NATIVE_MAX}"
                    )
            report.append(
                {
                    "id": entry["id"],
                    "name": entry_name(entry),
                    "model": entry["model"],
                    "triangles": mesh.triangle_count,
                    "vertices": len(mesh.positions),
                    "texture": pixels,
                    "decoded_texture_bytes": decoded,
                    "bytes": len(data),
                    "bounds_min": [round(value, 3) for value in low],
                    "bounds_max": [round(value, 3) for value in high],
                    "parts": [],
                    "notes": [],
                }
            )
        pack_decoded = sum(item["decoded_texture_bytes"] for item in report)
        if pack_decoded > PACK_TEXTURE_MEMORY_MAX:
            failures.append(
                f"the pack's decoded prop textures total {pack_decoded} bytes, over the "
                f"{PACK_TEXTURE_MEMORY_MAX}-byte desktop pack budget"
            )
        _print_report(report)
        for failure in failures:
            print(f"FAIL {failure}")
        return 1 if failures else 0

    for entry in entries:
        if args.only and entry["id"] not in args.only:
            continue
        if not entry.get("model"):
            print(f"skip {entry['id']}: catalogue entry declares no model")
            continue
        build_fn = registry.get(entry["id"])
        if build_fn is None:
            failures.append(f"{entry['id']}: no builder registered")
            continue
        try:
            report.append(build_one(entry, build_fn))
        except (ValueError, glb.GltfError) as error:
            failures.append(f"{entry['id']}: {error}")

    # The proxy file is merged entry by entry, so a `--only` build still
    # refreshes the props it rebuilt without touching the others.
    if not args.no_proxies and report:
        write_proxies(catalog, report, only=args.only)

    _print_report(report)
    if args.thumbs and report:
        import preview

        preview.render_thumbnails([item["id"] for item in report], THUMB_DIR)
    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        return 1
    print(f"\n{len(report)} prop(s) OK")
    return 0


def write_proxies(catalog: dict, report: List[dict], only: List[str] | None = None) -> None:
    """Writes the editor's derived proxy geometry (never hand-maintained)."""
    existing: Dict[str, dict] = {}
    if os.path.exists(PROXY_PATH):
        with open(PROXY_PATH, "r", encoding="utf-8") as handle:
            existing = json.load(handle).get("props", {})

    names = {entry["id"]: entry_name(entry) for entry in catalog_placeables(catalog)}
    for item in report:
        existing[item["id"]] = {
            "name": names.get(item["id"], item["name"]),
            "model": item["model"],
            "triangles": item["triangles"],
            "texture": item["texture"],
            "bounds_min": item["bounds_min"],
            "bounds_max": item["bounds_max"],
            "parts": item["parts"],
        }

    payload = {
        "format_version": 1,
        "generated_by": "tools/props/build.py",
        "note": "Derived from the shipped GLB meshes; do not edit by hand.",
        "props": existing,
    }
    with open(PROXY_PATH, "w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, sort_keys=True)
        handle.write("\n")


def _print_report(report: List[dict]) -> None:
    if not report:
        return
    header = f"{'prop':<24}{'tris':>6}{'verts':>7}{'tex':>10}{'glb':>9}  notes"
    print(header)
    print("-" * len(header))
    total_tris = 0
    total_bytes = 0
    total_decoded = 0
    for item in sorted(report, key=lambda entry: entry["id"]):
        texture = item["texture"]
        tex_label = f"{texture[0]}x{texture[1]}" if texture else "n/a"
        flag = ""
        if item["triangles"] > TRIANGLE_REVIEW:
            flag = " (review)"
        elif item["triangles"] > TRIANGLE_TARGET:
            flag = " (over target)"
        print(
            f"{item['id']:<24}{item['triangles']:>6}{item['vertices']:>7}{tex_label:>10}"
            f"{item['bytes'] / 1024.0:>8.1f}k{flag}"
        )
        total_tris += item["triangles"]
        total_bytes += item["bytes"]
        total_decoded += item.get("decoded_texture_bytes", 0)
    print("-" * len(header))
    print(
        f"{'total':<24}{total_tris:>6}{'':>7}{'':>10}{total_bytes / 1024.0:>8.1f}k  "
        f"({len(report)} props, {total_decoded / 1024.0:.1f} KiB decoded RGBA)"
    )
    print(
        f"budget: {TRIANGLE_TARGET} triangles preferred, {TRIANGLE_REVIEW} review, "
        f"{TRIANGLE_ART_MAX} shipped art max (engine {TRIANGLE_ENGINE_MAX}); "
        f"texture native {TEXTURE_NATIVE_MAX}x{TEXTURE_NATIVE_MAX} "
        f"(engine {TEXTURE_ENGINE_MAX}, Full samples {TEXTURE_NATIVE_MAX} / Low 128); "
        f"pack decoded memory <= {PACK_TEXTURE_MEMORY_MAX // (1024 * 1024)} MiB"
    )


if __name__ == "__main__":
    raise SystemExit(main())

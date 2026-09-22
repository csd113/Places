#!/usr/bin/env python3
"""Authors and validates the environment surface PNGs.

The shipped PNGs are the authoritative runtime assets: the game loads them at
level load and never runs this script, and replacing a PNG needs no Rust change
and no recompilation.  This tool exists so the built-in artwork can be
regenerated deterministically from source, and so its dimensions stay inside
the texture budget.

The painters live in per-theme modules (``office_art.py``, ``pool_art.py``), the
fixture family (``lights_art.py``) and in ``decal_art.py`` /
``diagnostic_art.py``; ``build.py`` owns the CLI, the manifest merge and the
``--check`` gate.

Run it from the repository root::

    python3 tools/textures/build.py           # (re)generate every manifest texture
    python3 tools/textures/build.py --check   # validate the shipped PNGs only
    python3 tools/textures/build.py --only core:tex_pool_tile_deck_01

``--check`` never regenerates: it reads the catalog, parses each file-backed
sheet PNG's IHDR (surface textures, decal sheets and fixture faces) and fails on
missing/corrupt/oversized files (hard limit 1024x1024, preferred 256x256,
power-of-two dimensions preferred).
"""

from __future__ import annotations

import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGE_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASSET_ROOT = os.path.join(PACKAGE_ROOT, "assets")
CATALOG_PATH = os.path.join(ASSET_ROOT, "catalog.json")

if HERE not in sys.path:
    sys.path.insert(0, HERE)

from artkit import PNG_SIGNATURE, write_png  # noqa: E402
import decal_art  # noqa: E402
import diagnostic_art  # noqa: E402
import lights_art  # noqa: E402
import office_art  # noqa: E402
import pool_art  # noqa: E402

# Budgets for the ES 2.0 / Mali-400 target.  256x256 is preferred; 1024x1024
# is the hard ceiling (see the README and assets/README.md).
PREFERRED_DIMENSION = 256
HARD_DIMENSION = 1024

# id -> (catalog model path relative to assets/, painter).  Built from the
# per-theme art modules; mirrors the asset_type "texture" entries in
# assets/catalog.json and --check warns on drift.
MANIFEST = {}
for module in (office_art, pool_art, lights_art, decal_art, diagnostic_art):
    for texture_id, entry in module.ART.items():
        if texture_id in MANIFEST:
            raise SystemExit(f"texture manifest: {texture_id} is declared in more than one art module")
        MANIFEST[texture_id] = entry


# ---------------------------------------------------------------- validation


def is_power_of_two(value: int) -> bool:
    return value > 0 and (value & (value - 1)) == 0


def read_png_dimensions(path: str) -> tuple[int, int]:
    """Parses a PNG's signature and IHDR, returning ``(width, height)``."""
    with open(path, "rb") as handle:
        data = handle.read()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("missing PNG signature")
    if len(data) < 33:
        raise ValueError("truncated PNG")
    length = int.from_bytes(data[8:12], "big")
    tag = data[12:16]
    if tag != b"IHDR" or length != 13:
        raise ValueError("the first chunk is not a 13-byte IHDR")
    if data[-8:-4] != b"IEND":
        raise ValueError("missing IEND chunk")
    return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")


def validate_textures(
    catalog_path: str = CATALOG_PATH, asset_root: str = ASSET_ROOT
) -> tuple[list[str], list[str], list[str]]:
    """Returns ``(errors, warnings, report)`` for the catalog's texture assets."""
    errors: list[str] = []
    warnings: list[str] = []
    report: list[str] = []
    try:
        with open(catalog_path, "r", encoding="utf-8") as handle:
            catalog = json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        return [f"catalog: {error}"], warnings, report

    catalog_ids: list[str] = []
    for entry in catalog.get("assets", []):
        asset_type = entry.get("asset_type")
        # Every catalogued sheet the renderer loads at level load is checked:
        # surface textures, decal sheets and the visible face of a fixture.
        # They are all file-backed PNGs below the asset root.
        file_sheet = entry.get("source") == "file" and asset_type in (
            "texture",
            "decal",
            "light",
        )
        if not file_sheet:
            continue
        texture_id = str(entry.get("id", "")).strip() or "texture"
        catalog_ids.append(str(entry.get("id", "")).strip())
        model = entry.get("model")
        if not isinstance(model, str) or not model.strip():
            errors.append(f"{texture_id}: texture asset has no model path")
            continue
        model = model.strip()
        path = os.path.join(asset_root, model)
        if not os.path.isfile(path):
            errors.append(f"{texture_id}: file '{model}' is missing below assets/")
            continue
        try:
            width, height = read_png_dimensions(path)
        except (OSError, ValueError) as error:
            errors.append(f"{texture_id}: '{model}' is not a valid PNG ({error})")
            continue
        if width <= 0 or height <= 0:
            errors.append(f"{texture_id}: '{model}' has zero pixels ({width}x{height})")
            continue
        if width > HARD_DIMENSION or height > HARD_DIMENSION:
            errors.append(
                f"{texture_id}: '{model}' is {width}x{height}, over the {HARD_DIMENSION}x{HARD_DIMENSION} hard limit"
            )
        if width > PREFERRED_DIMENSION or height > PREFERRED_DIMENSION:
            warnings.append(
                f"{texture_id}: '{model}' is {width}x{height}, over the preferred {PREFERRED_DIMENSION}x{PREFERRED_DIMENSION}"
            )
        if not is_power_of_two(width) or not is_power_of_two(height):
            warnings.append(f"{texture_id}: '{model}' is {width}x{height}, not a power of two")
        report.append(f"OK {texture_id}: {model} {width}x{height}")

    catalog_set = set(catalog_ids)
    manifest_set = set(MANIFEST)
    for texture_id in sorted(catalog_set - manifest_set):
        warnings.append(f"manifest: '{texture_id}' is in the catalog but not in tools/textures/")
    for texture_id in sorted(manifest_set - catalog_set):
        warnings.append(f"manifest: '{texture_id}' is in tools/textures/ but not the catalog")
    return errors, warnings, report


# ---------------------------------------------------------------------- main


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="validate the shipped PNGs without regenerating them")
    parser.add_argument("--only", nargs="+", metavar="ID", help="regenerate only these logical texture ids")
    parser.add_argument("--quiet", action="store_true", help="only print problems")
    args = parser.parse_args(argv)

    selected = sorted(MANIFEST)
    if args.only:
        unknown = [texture_id for texture_id in args.only if texture_id not in MANIFEST]
        if unknown:
            for texture_id in unknown:
                print(f"FAIL {texture_id}: not in the texture manifest")
            return 1
        selected = sorted(set(args.only))

    if not args.check:
        for texture_id in selected:
            entry = MANIFEST[texture_id]
            path = os.path.join(ASSET_ROOT, entry["model"])
            canvas = entry["build"]()
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as handle:
                handle.write(write_png(canvas.width, canvas.height, canvas.rgba()))
            if not args.quiet:
                print(f"wrote assets/{entry['model']} ({canvas.width}x{canvas.height})")

    errors, warnings, report = validate_textures()
    if not args.quiet:
        for line in report:
            print(line)
    for warning in warnings:
        print(f"WARN {warning}")
    for error in errors:
        print(f"FAIL {error}")
    if errors:
        print(f"\n{len(errors)} error(s), {len(warnings)} warning(s)")
        return 1
    if not args.quiet:
        print(f"OK ({len(report)} texture(s), {len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

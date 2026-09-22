#!/usr/bin/env python3
"""Validates the Places asset catalog and the levels that reference it.

The checks here are the tooling half of the asset architecture:

* the catalog at ``assets/catalog.json`` parses and declares the built-in
  environment themes (``office``, ``pool``) with well-formed identifiers;
* every logical asset id is unique and well-formed, and every asset declares a
  known class (``environment``, ``entity``, ``core``, ``diagnostic``) and type
  (``prop``, ``material``, ``texture``, ``light``, ``decal``, ``entity``);
* file-backed assets name a relative resource path that exists exactly once
  below ``assets/``, and generated assets never name a file;
* ``spooner-man`` is a single canonical entity resource under
  ``entities/spooner-man/``, never a duplicate prop file;
* every shipped level in ``assets/levels/`` and every custom level in
  ``levels/`` only references ids the catalog declares.

Run it from the repository root::

    python3 tools/assets/validate.py

It exits non-zero on the first class of problem and prints every failure, so it
can gate packaging and continuous checks. ``tests/test_package.py`` imports
``validate_catalog`` and re-runs the same checks under ``cargo test``-adjacent
tooling.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from typing import Dict, List, Tuple

HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGE_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASSET_ROOT = os.path.join(PACKAGE_ROOT, "assets")
CATALOG_PATH = os.path.join(ASSET_ROOT, "catalog.json")
LEVEL_DIRS = (
    os.path.join(ASSET_ROOT, "levels"),
    os.path.join(PACKAGE_ROOT, "levels"),
)

# Architectural classification: adding a class is deliberate (it changes what
# tooling understands), while themes are pure data and extend freely.
KNOWN_CLASSES = {"environment", "entity", "core", "diagnostic"}
KNOWN_TYPES = {"prop", "material", "texture", "light", "decal", "entity"}
PLACEABLE_TYPES = {"prop", "entity"}
BUILTIN_THEMES = {"office", "pool"}

_SLUG = re.compile(r"^[a-z][a-z0-9_-]*$")
_ASSET_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9:_-]*$")


def load_catalog(path: str = CATALOG_PATH) -> dict:
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def catalog_entries(catalog: dict) -> List[dict]:
    """Every entry, accepting the legacy ``props`` array for old tooling."""
    entries = catalog.get("assets")
    if entries is None:
        entries = catalog.get("props", [])
    return list(entries)


def placeable_entries(catalog: dict) -> List[dict]:
    return [
        entry
        for entry in catalog_entries(catalog)
        if entry.get("asset_type", "prop") in PLACEABLE_TYPES
    ]


def is_relative_resource(path: str) -> bool:
    if not path or path.startswith("/") or "\\" in path or ":" in path:
        return False
    return all(component not in ("", ".", "..") for component in path.split("/"))


def validate_catalog(catalog: dict, asset_root: str = ASSET_ROOT) -> Tuple[List[str], List[str]]:
    """Returns ``(errors, warnings)`` for one parsed catalog document."""
    errors: List[str] = []
    warnings: List[str] = []

    themes = catalog.get("themes") or []
    theme_ids: List[str] = []
    for theme in themes:
        theme_id = str(theme.get("id", "")).strip()
        if not theme_id:
            errors.append("themes: an entry has an empty id")
            continue
        if not _SLUG.match(theme_id):
            errors.append(f"themes: '{theme_id}' is not a valid theme identifier")
            continue
        if theme_id in theme_ids:
            errors.append(f"themes: duplicate theme id '{theme_id}'")
            continue
        theme_ids.append(theme_id)
        if not str(theme.get("display_name") or theme.get("name") or "").strip():
            warnings.append(f"themes: '{theme_id}' has no display_name")
    for required in sorted(BUILTIN_THEMES):
        if required not in theme_ids:
            errors.append(f"themes: the built-in '{required}' environment theme is missing")

    seen_ids: Dict[str, int] = {}
    seen_models: Dict[str, str] = {}
    for index, entry in enumerate(catalog_entries(catalog)):
        raw_id = str(entry.get("id", "")).strip()
        where = raw_id or f"entry #{index + 1}"
        if not raw_id:
            errors.append(f"{where}: asset entry has an empty id")
            continue
        if not _ASSET_ID.match(raw_id):
            errors.append(f"{where}: malformed asset id")
            continue
        if raw_id in seen_ids:
            errors.append(f"{where}: duplicate logical asset id")
            continue
        seen_ids[raw_id] = index

        asset_class = str(entry.get("asset_class", "")).strip()
        if not asset_class:
            errors.append(f"{where}: missing asset_class")
        elif not _SLUG.match(asset_class):
            errors.append(f"{where}: invalid asset_class '{asset_class}'")
        elif asset_class not in KNOWN_CLASSES:
            errors.append(
                f"{where}: unknown asset_class '{asset_class}' "
                f"(known: {', '.join(sorted(KNOWN_CLASSES))})"
            )

        asset_type = str(entry.get("asset_type", "")).strip()
        if not asset_type:
            errors.append(f"{where}: missing asset_type")
        elif not _SLUG.match(asset_type):
            errors.append(f"{where}: invalid asset_type '{asset_type}'")
        elif asset_type not in KNOWN_TYPES:
            errors.append(
                f"{where}: unknown asset_type '{asset_type}' "
                f"(known: {', '.join(sorted(KNOWN_TYPES))})"
            )

        theme = entry.get("theme")
        if theme is not None:
            theme = str(theme).strip()
            if not theme:
                errors.append(f"{where}: empty theme identifier")
            elif not _SLUG.match(theme):
                errors.append(f"{where}: invalid theme identifier '{theme}'")
            elif theme not in theme_ids:
                warnings.append(
                    f"{where}: theme '{theme}' is not declared in the catalog's themes list"
                )

        source = str(entry.get("source", "")).strip()
        model = entry.get("model")
        model = str(model).strip() if isinstance(model, str) else None
        if source and source not in ("file", "generated"):
            errors.append(f"{where}: source must be 'file' or 'generated'")
        if model:
            if not is_relative_resource(model):
                errors.append(f"{where}: model path '{model}' must be relative to assets/")
            elif not os.path.isfile(os.path.join(asset_root, model)):
                errors.append(f"{where}: model file '{model}' does not exist below assets/")
            elif model in seen_models:
                errors.append(f"{where}: model '{model}' is already claimed by {seen_models[model]}")
            else:
                seen_models[model] = raw_id
            if source == "generated":
                errors.append(f"{where}: a generated asset must not declare a model")
        elif source == "file" or (not source and asset_type in PLACEABLE_TYPES):
            errors.append(f"{where}: a file asset must declare a model")
        elif asset_type in PLACEABLE_TYPES and not model:
            errors.append(f"{where}: placeable assets must ship a model")

        if asset_type in PLACEABLE_TYPES:
            size = entry.get("size")
            if not (isinstance(size, list) and len(size) == 3 and all(isinstance(v, (int, float)) and v > 0 for v in size)):
                errors.append(f"{where}: placeable assets need a positive [width, height, depth] size")

    # The canonical entity resource: one logical id, one physical file.
    spooner_entries = [entry for entry in catalog_entries(catalog) if str(entry.get("id")) == "spooner-man"]
    if len(spooner_entries) != 1:
        errors.append("spooner-man: exactly one catalog entry must define the logical id 'spooner-man'")
    else:
        spooner = spooner_entries[0]
        if str(spooner.get("asset_class")) != "entity":
            errors.append("spooner-man: asset_class must be 'entity', not a theme or a prop")
        if str(spooner.get("asset_type")) != "entity":
            errors.append("spooner-man: asset_type must be 'entity'")
        model = str(spooner.get("model", ""))
        if not model.startswith("entities/spooner-man/"):
            errors.append("spooner-man: the canonical resource must live under entities/spooner-man/")
        else:
            canonical = os.path.join(asset_root, model)
            if not os.path.isfile(canonical):
                errors.append(f"spooner-man: canonical resource '{model}' is missing")
            duplicate = os.path.join(asset_root, "props", "models", "spooner-man.glb")
            if os.path.isfile(duplicate):
                errors.append("spooner-man: a duplicate legacy copy exists at assets/props/models/spooner-man.glb")
        if spooner.get("theme") is not None:
            errors.append("spooner-man: an entity must not carry an environment theme")

    return errors, warnings


def level_ids(level: dict):
    """Yields ``(id, what)`` for every asset reference in a parsed level."""
    defaults = level.get("defaults") or {}
    for key in ("wall", "floor", "ceiling"):
        if defaults.get(key):
            yield str(defaults[key]), f"defaults.{key}"
    # Both spellings: `rooms` is the list, `room` an optional single room.
    rooms = list(level.get("rooms") or [])
    if level.get("room"):
        rooms.append(level["room"])
    for room in rooms:
        if room.get("material"):
            yield str(room["material"]), "room floor material"
        if room.get("ceiling_material"):
            yield str(room["ceiling_material"]), "room ceiling material"
    for wall in level.get("walls") or []:
        if wall.get("material"):
            yield str(wall["material"]), "wall material"
        for face, material in (wall.get("faces") or {}).items():
            yield str(material), f"wall {face} face material"
    for patch in level.get("floor_patches") or []:
        if patch.get("material"):
            yield str(patch["material"]), "floor patch material"
    for light in level.get("ceiling_lights") or []:
        if light.get("fixture"):
            yield str(light["fixture"]), "ceiling light fixture"
    for decal in level.get("decals") or []:
        if decal.get("material"):
            yield str(decal["material"]), "decal sheet"
    for prop in level.get("props") or []:
        if prop.get("model"):
            yield str(prop["model"]), "prop"


def validate_levels(catalog: dict) -> Tuple[List[str], List[str]]:
    """Returns ``(errors, warnings)`` for every shipped and custom level."""
    errors: List[str] = []
    warnings: List[str] = []
    known = {str(entry.get("id")) for entry in catalog_entries(catalog)}
    placeable = {str(entry.get("id")) for entry in placeable_entries(catalog)}
    levels = 0
    for directory in LEVEL_DIRS:
        if not os.path.isdir(directory):
            warnings.append(f"levels: directory '{os.path.relpath(directory, PACKAGE_ROOT)}' is missing")
            continue
        for name in sorted(os.listdir(directory)):
            if not name.endswith(".json"):
                continue
            path = os.path.join(directory, name)
            levels += 1
            with open(path, "r", encoding="utf-8") as handle:
                level = json.load(handle)
            for asset_id, what in level_ids(level):
                if asset_id not in known:
                    errors.append(f"{os.path.relpath(path, PACKAGE_ROOT)}: {what} '{asset_id}' is not in the catalog")
                elif what == "prop" and asset_id not in placeable:
                    errors.append(f"{os.path.relpath(path, PACKAGE_ROOT)}: prop '{asset_id}' is not a placeable asset")
    if not levels:
        errors.append("levels: no level JSON files were found to validate")
    return errors, warnings


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--catalog", default=CATALOG_PATH, help="catalog path (defaults to assets/catalog.json)")
    parser.add_argument("--quiet", action="store_true", help="only print problems")
    args = parser.parse_args(argv)

    try:
        catalog = load_catalog(args.catalog)
    except (OSError, json.JSONDecodeError) as error:
        print(f"FAIL {os.path.relpath(args.catalog, PACKAGE_ROOT)}: {error}")
        return 1

    catalog_errors, catalog_warnings = validate_catalog(catalog)
    level_errors, level_warnings = validate_levels(catalog)
    errors = catalog_errors + level_errors
    warnings = catalog_warnings + level_warnings

    if not args.quiet:
        assets = catalog_entries(catalog)
        print(
            f"catalog: {len(assets)} assets "
            f"({len(placeable_entries(catalog))} placeable), "
            f"{len(catalog.get('themes') or [])} themes"
        )
    for warning in warnings:
        print(f"WARN {warning}")
    for error in errors:
        print(f"FAIL {error}")
    if errors:
        print(f"\n{len(errors)} error(s), {len(warnings)} warning(s)")
        return 1
    if not args.quiet:
        print(f"OK ({len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

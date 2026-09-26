#!/usr/bin/env python3
"""Validates the Places asset catalog and the levels that reference it.

The checks here are the tooling half of the asset architecture:

* the catalog at ``assets/catalog.json`` parses and declares the built-in
  environment themes (``office``, ``pool``) with well-formed identifiers;
* every logical asset id is unique and well-formed, and every asset declares a
  known class (``environment``, ``entity``, ``core``, ``diagnostic``) and type
  (``prop``, ``material``, ``texture``, ``light``, ``decal``, ``entity``);
* file-backed assets name a relative resource path that exists exactly once
  below ``assets/`` (a GLB for a placeable, a PNG for a texture, a decal sheet
  or a fixture face), generated assets never name a file, and definition
  assets (materials) resolve to a file-backed PNG texture instead;
* definition materials may author emission (``emissive``, ``emissive_intensity``
  and an ``emissive_mask`` that resolves to a file-backed PNG texture exactly
  like the material's own ``texture``);
* ``spooner-man`` is a single canonical entity resource under
  ``entities/spooner-man/``, never a duplicate prop file;
* the shipped level in ``assets/levels/``, the drop-in levels in ``levels/``
  and the engine regression fixtures in ``tests/fixtures/levels/`` only
  reference ids the catalog declares, and their optional ``ceiling_lights[]``
  pool/emission fields, ``ceiling_lights[].enabled`` switch and
  ``props[].lights`` sources obey the documented shapes, dimensions and ranges.

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
import math
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
    os.path.join(PACKAGE_ROOT, "tests", "fixtures", "levels"),
)

# Architectural classification: adding a class is deliberate (it changes what
# tooling understands), while themes are pure data and extend freely.
KNOWN_CLASSES = {"environment", "entity", "core", "diagnostic"}
KNOWN_TYPES = {"prop", "material", "texture", "light", "decal", "entity"}
PLACEABLE_TYPES = {"prop", "entity"}
BUILTIN_THEMES = {"office", "pool"}

# Generic light sources a prop may own: a dimension-free "point", a "rect"
# sized by half extents and a "line" sized by its length. The engine's default
# shape is "rect" when half extents are authored, otherwise "point".
LIGHT_SHAPES = ("point", "rect", "line")
LIGHT_FALLOFFS = ("smooth", "linear", "constant")
# An authored prop-light intensity above this still loads: the engine clamps it.
MAX_LIGHT_INTENSITY = 8.0
# Emissive materials: colour channels are 0..1 and the intensity is clamped by
# the engine (mirrors src/materials/emission.rs).
MAX_EMISSION_INTENSITY = 8.0

# Map-authored actions: the closed set the engine parses, and the subset it
# implements. The reserved tags are rejected by name, not silently ignored.
ACTION_TAGS = (
    "toggle_label",
    "reset_to_start",
    "play_animation",
    "toggle_animation",
    "play_audio",
)
IMPLEMENTED_ACTIONS = (
    "toggle_label",
    "reset_to_start",
    "play_animation",
    "toggle_animation",
)

# Mirrors src/level.rs: a float's motion is bounded and must be contained by
# the water volume it rides.
MAX_LEVEL_FLOAT_PROPS = 32
MAX_FLOAT_HEEL_DEGREES = 45.0
# Bounds mirror src/level.rs (MAX_ACTIONS_PER_SOURCE, MAX_LEVEL_AREA_TRIGGERS,
# MAX_LEVEL_ROUTES, MAX_ROUTE_STEPS, MAX_ROUTE_SPEED_MPS, MAX_ROUTE_WAIT_SECONDS,
# MAX_ROUTE_PLAY_SECONDS) and src/interact.rs (MAX_INTERACTION_REACH_M).
MAX_ACTIONS_PER_SOURCE = 8
MAX_AREA_TRIGGERS = 1000
MAX_INTERACTION_REACH_M = 4.0
MAX_ENTITY_ROUTES = 256
MAX_ROUTE_STEPS = 64
MAX_ROUTE_SPEED_MPS = 6.0
MAX_ROUTE_SECONDS = 3600.0
ROUTE_STEP_TAGS = ("move_to", "face", "wait", "play")

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


def is_finite_number(value: object) -> bool:
    """True for a JSON number that is real (never a bool) and finite."""
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def is_color_triplet(value: object) -> bool:
    """True for ``[r, g, b]`` with each channel a finite number in 0..1."""
    return (
        isinstance(value, list)
        and len(value) == 3
        and all(is_finite_number(channel) and 0.0 <= channel <= 1.0 for channel in value)
    )


def validate_light_source(light: object, where: str) -> Tuple[List[str], List[str]]:
    """Validates one generic light source attached to a prop.

    Mirrors the engine's prop-light schema: ``shape`` is ``point``/``rect``/
    ``line`` (defaulting to ``rect`` when half extents are authored, else
    ``point``), a rect needs a positive ``half_width``/``half_depth``, a line
    needs a positive ``length``, and every optional placement field is a finite
    number in its documented range. Returns ``(errors, warnings)`` with every
    message prefixed by ``where``; intensities above the engine clamp warn
    instead of failing.
    """
    errors: List[str] = []
    warnings: List[str] = []
    if not isinstance(light, dict):
        errors.append(f"{where}: must be an object")
        return errors, warnings

    shape = light.get("shape")
    if shape is None:
        if light.get("half_width") is not None or light.get("half_depth") is not None:
            shape = "rect"
        elif light.get("length") is not None:
            shape = "line"
        else:
            shape = "point"
    elif not isinstance(shape, str) or shape not in LIGHT_SHAPES:
        errors.append(f"{where}: shape must be one of {', '.join(LIGHT_SHAPES)}")
    if shape == "rect":
        for field in ("half_width", "half_depth"):
            value = light.get(field)
            if value is None:
                errors.append(f"{where}: a rect light needs {field}")
            elif not is_finite_number(value) or value <= 0.0:
                errors.append(f"{where}: {field} must be a finite number > 0")
    elif shape == "line":
        value = light.get("length")
        if value is None:
            errors.append(f"{where}: a line light needs length")
        elif not is_finite_number(value) or value <= 0.0:
            errors.append(f"{where}: length must be a finite number > 0")

    offset = light.get("offset")
    if offset is not None:
        valid_offset = (
            isinstance(offset, list)
            and len(offset) == 3
            and all(is_finite_number(component) for component in offset)
        )
        if not valid_offset:
            errors.append(f"{where}: offset must be exactly three finite numbers")
    if light.get("color") is not None and not is_color_triplet(light.get("color")):
        errors.append(f"{where}: color must be three numbers in 0..1")
    rotation = light.get("rotation_degrees")
    if rotation is not None and not is_finite_number(rotation):
        errors.append(f"{where}: rotation_degrees must be a finite number")
    for field in ("intensity", "brightness"):
        value = light.get(field)
        if value is None:
            continue
        if not is_finite_number(value):
            errors.append(f"{where}: {field} must be a finite number >= 0")
        elif value < 0.0:
            errors.append(f"{where}: {field} cannot be negative")
        elif value > MAX_LIGHT_INTENSITY:
            warnings.append(
                f"{where}: {field} {value} is above {MAX_LIGHT_INTENSITY} and will be clamped by the engine"
            )
    light_range = light.get("range")
    if light_range is not None and (not is_finite_number(light_range) or light_range <= 0.0):
        errors.append(f"{where}: range must be a finite number > 0")
    falloff = light.get("falloff")
    if falloff is not None and (not isinstance(falloff, str) or falloff not in LIGHT_FALLOFFS):
        errors.append(f"{where}: falloff must be one of {', '.join(LIGHT_FALLOFFS)}")
    if "enabled" in light and not isinstance(light.get("enabled"), bool):
        errors.append(f"{where}: enabled must be a boolean")
    return errors, warnings


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
    material_textures: List[Tuple[str, str]] = []
    material_emissive_masks: List[Tuple[str, str]] = []
    material_normal_textures: List[Tuple[str, str]] = []
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
        texture_id = entry.get("texture")
        texture_id = texture_id.strip() if isinstance(texture_id, str) else None
        if source and source not in ("file", "generated", "definition"):
            errors.append(f"{where}: source must be 'file', 'generated' or 'definition'")
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
            if source == "definition":
                errors.append(f"{where}: a definition asset must not declare a model")
            # A light's mesh is generated geometry, so its `model` is not a GLB:
            # it is the PNG sheet of the fixture's visible face, exactly like a
            # file-backed decal's `model`.
            if asset_type == "light" and not model.lower().endswith(".png"):
                errors.append(f"{where}: a light fixture must name a .png sheet, found '{model}'")
        elif source == "file" or (not source and asset_type in PLACEABLE_TYPES):
            errors.append(f"{where}: a file asset must declare a model")
        elif asset_type in PLACEABLE_TYPES and not model:
            errors.append(f"{where}: placeable assets must ship a model")

        if asset_type == "material" and not texture_id:
            errors.append(f"{where}: a material must declare a texture")
        elif source == "definition" and not texture_id:
            errors.append(f"{where}: a definition asset must declare a texture")
        if asset_type != "material":
            for field in ("texture", "tile_metres", "tint"):
                if field in entry:
                    errors.append(f"{where}: only a material may declare {field}")
        if texture_id:
            if not _ASSET_ID.match(texture_id):
                errors.append(f"{where}: malformed texture id '{texture_id}'")
            elif asset_type == "material":
                material_textures.append((raw_id, texture_id))
        tile_metres = entry.get("tile_metres")
        if tile_metres is not None:
            valid_tile = (
                isinstance(tile_metres, (int, float))
                and not isinstance(tile_metres, bool)
                and math.isfinite(tile_metres)
                and 0.05 <= tile_metres <= 64.0
            )
            if not valid_tile:
                errors.append(f"{where}: tile_metres must be a number between 0.05 and 64")
        tint = entry.get("tint")
        if tint is not None:
            valid_tint = (
                isinstance(tint, list)
                and len(tint) == 3
                and all(
                    isinstance(v, (int, float))
                    and not isinstance(v, bool)
                    and math.isfinite(v)
                    and 0.0 <= v <= 1.0
                    for v in tint
                )
            )
            if not valid_tint:
                errors.append(f"{where}: tint must be three numbers in 0..1")
        surface = entry.get("surface")
        if surface is not None and str(surface).strip() not in ("wall", "floor", "ceiling"):
            errors.append(f"{where}: surface must be 'wall', 'floor' or 'ceiling'")

        # Surface-response and alpha fields. Like emission these are material
        # definition fields: a prop or a texture that authored them would be a
        # silent no-op, so it is an error.
        normal_texture = entry.get("normal_texture")
        normal_strength = entry.get("normal_strength")
        specular = entry.get("specular")
        specular_color = entry.get("specular_color")
        shine = entry.get("shine")
        roughness = entry.get("roughness")
        alpha_mode = entry.get("alpha_mode")
        opacity = entry.get("opacity")
        alpha_cutoff = entry.get("alpha_cutoff")
        reflection_mode = entry.get("reflection_mode")
        reflection_strength = entry.get("reflection_strength")
        declares_response = any(
            entry.get(field) is not None
            for field in (
                "normal_texture",
                "normal_strength",
                "specular",
                "specular_color",
                "shine",
                "roughness",
                "alpha_mode",
                "opacity",
                "alpha_cutoff",
                "reflection_mode",
                "reflection_strength",
            )
        )
        if declares_response and not (asset_type == "material" and source == "definition"):
            errors.append(
                f"{where}: surface-response and alpha fields are only valid on a definition material"
            )
        else:
            if normal_texture is not None:
                normal_id = str(normal_texture).strip()
                if not _ASSET_ID.match(normal_id):
                    errors.append(f"{where}: malformed normal_texture id '{normal_id}'")
                else:
                    material_normal_textures.append((raw_id, normal_id))
            if normal_strength is not None and (
                not is_finite_number(normal_strength) or not 0.0 <= normal_strength <= 2.0
            ):
                errors.append(f"{where}: normal_strength must be a number between 0 and 2")
            if normal_strength is not None and normal_texture is None:
                errors.append(f"{where}: normal_strength requires a normal_texture")
            if specular is not None and (
                not is_finite_number(specular) or not 0.0 <= specular <= 1.0
            ):
                errors.append(f"{where}: specular must be a number between 0 and 1")
            if specular_color is not None and not is_color_triplet(specular_color):
                errors.append(f"{where}: specular_color must be three numbers in 0..1")
            if shine is not None and (
                not is_finite_number(shine) or not 0.0 <= shine <= 1.0
            ):
                errors.append(f"{where}: shine must be a number between 0 and 1")
            if roughness is not None and (
                not is_finite_number(roughness) or not 0.0 <= roughness <= 1.0
            ):
                errors.append(f"{where}: roughness must be a number between 0 and 1")
            if shine is not None and roughness is not None:
                errors.append(
                    f"{where}: author either shine or roughness, not both "
                    "(shine is the author-facing spelling; roughness is its inverse)"
                )
            if alpha_mode is not None:
                mode = str(alpha_mode).strip().lower()
                if mode not in ("opaque", "cutout", "blend"):
                    errors.append(
                        f"{where}: alpha_mode must be 'opaque', 'cutout' or 'blend', found '{alpha_mode}'"
                    )
            for field, value in (("opacity", opacity), ("alpha_cutoff", alpha_cutoff)):
                if value is None:
                    continue
                if not is_finite_number(value) or not 0.0 <= value <= 1.0:
                    errors.append(f"{where}: {field} must be a number between 0 and 1")
                if alpha_mode is None:
                    errors.append(f"{where}: {field} requires an explicit alpha_mode")
            # Selective reflections: a mode plus its weight.
            if reflection_mode is not None and str(reflection_mode).strip().lower() not in (
                "none",
                "probe",
                "planar",
            ):
                errors.append(
                    f"{where}: reflection_mode must be 'none', 'probe' or 'planar', "
                    f"found '{reflection_mode}'"
                )
            if reflection_strength is not None and (
                not is_finite_number(reflection_strength)
                or not 0.0 <= reflection_strength <= 1.0
            ):
                errors.append(f"{where}: reflection_strength must be a number between 0 and 1")
            if reflection_strength is not None and reflection_mode is None:
                errors.append(f"{where}: reflection_strength requires an explicit reflection_mode")

        # Material emission: a colour triple, an intensity and an optional mask
        # texture. Only definition materials may author it, and the mask must
        # resolve to a file-backed PNG texture exactly like the material's own
        # `texture` (checked with the other cross-entry references below).
        emissive = entry.get("emissive")
        emissive_intensity = entry.get("emissive_intensity")
        emissive_mask = entry.get("emissive_mask")
        if asset_type == "material" and source == "definition":
            if emissive is not None and not is_color_triplet(emissive):
                errors.append(f"{where}: emissive must be three numbers in 0..1")
            if emissive_intensity is not None:
                valid_emissive_intensity = (
                    is_finite_number(emissive_intensity)
                    and 0.0 <= emissive_intensity <= MAX_EMISSION_INTENSITY
                )
                if not valid_emissive_intensity:
                    errors.append(
                        f"{where}: emissive_intensity must be a number between 0 and {MAX_EMISSION_INTENSITY:g}"
                    )
            if emissive_mask is not None:
                mask_id = str(emissive_mask).strip()
                if not _ASSET_ID.match(mask_id):
                    errors.append(f"{where}: malformed emissive_mask id '{mask_id}'")
                else:
                    material_emissive_masks.append((raw_id, mask_id))
            if emissive is None:
                if emissive_intensity is not None:
                    errors.append(f"{where}: emissive_intensity requires emissive")
                if emissive_mask is not None:
                    errors.append(f"{where}: emissive_mask requires emissive")
        else:
            for field in ("emissive", "emissive_intensity", "emissive_mask"):
                if entry.get(field) is not None:
                    errors.append(f"{where}: {field} is only valid on a definition material")

        if asset_type in PLACEABLE_TYPES:
            size = entry.get("size")
            if not (isinstance(size, list) and len(size) == 3 and all(isinstance(v, (int, float)) and v > 0 for v in size)):
                errors.append(f"{where}: placeable assets need a positive [width, height, depth] size")

    # Every material resolves to a real, file-backed PNG texture.  The catalog
    # is the only lookup: a material never carries a physical path itself.
    entries_by_id: Dict[str, dict] = {}
    for entry in catalog_entries(catalog):
        entry_id = str(entry.get("id", "")).strip()
        if entry_id in seen_ids:
            entries_by_id[entry_id] = entry
    for material_id, texture_id in material_textures:
        target = entries_by_id.get(texture_id)
        if target is None:
            errors.append(f"{material_id}: texture '{texture_id}' is not in the catalog")
            continue
        if str(target.get("asset_type", "")).strip() != "texture":
            errors.append(f"{material_id}: texture '{texture_id}' is not a texture asset")
            continue
        target_source = str(target.get("source", "")).strip()
        target_model = target.get("model")
        target_model = str(target_model).strip() if isinstance(target_model, str) else ""
        if target_source != "file":
            errors.append(f"{material_id}: texture '{texture_id}' must be a file asset")
        elif not target_model.lower().endswith(".png"):
            errors.append(f"{material_id}: texture '{texture_id}' must name a .png model")
        elif not os.path.isfile(os.path.join(asset_root, target_model)):
            errors.append(
                f"{material_id}: texture '{texture_id}' file '{target_model}' does not exist below assets/"
            )

    # A normal map follows the same contract as the material's texture: a
    # catalog texture entry backed by a real .png below assets/.
    for material_id, normal_id in material_normal_textures:
        target = entries_by_id.get(normal_id)
        if target is None:
            errors.append(f"{material_id}: normal_texture '{normal_id}' is not in the catalog")
            continue
        if str(target.get("asset_type", "")).strip() != "texture":
            errors.append(f"{material_id}: normal_texture '{normal_id}' is not a texture asset")
            continue
        if str(target.get("source", "")).strip() != "file":
            errors.append(f"{material_id}: normal_texture '{normal_id}' must be a file asset")
            continue
        target_model = str(target.get("model", "")).strip()
        if not target_model.lower().endswith(".png"):
            errors.append(f"{material_id}: normal_texture '{normal_id}' must name a .png model")
            continue
        if not os.path.isfile(os.path.join(asset_root, target_model)):
            errors.append(
                f"{material_id}: normal_texture '{normal_id}' file '{target_model}' does not exist below assets/"
            )

    # An emissive mask follows the same contract as the material's texture: a
    # catalog texture entry backed by a real .png below assets/.
    for material_id, mask_id in material_emissive_masks:
        target = entries_by_id.get(mask_id)
        if target is None:
            errors.append(f"{material_id}: emissive_mask '{mask_id}' is not in the catalog")
            continue
        if str(target.get("asset_type", "")).strip() != "texture":
            errors.append(f"{material_id}: emissive_mask '{mask_id}' is not a texture asset")
            continue
        target_source = str(target.get("source", "")).strip()
        target_model = target.get("model")
        target_model = str(target_model).strip() if isinstance(target_model, str) else ""
        if target_source != "file":
            errors.append(f"{material_id}: emissive_mask '{mask_id}' must be a file asset")
        elif not target_model.lower().endswith(".png"):
            errors.append(f"{material_id}: emissive_mask '{mask_id}' must name a .png model")
        elif not os.path.isfile(os.path.join(asset_root, target_model)):
            errors.append(
                f"{material_id}: emissive_mask '{mask_id}' file '{target_model}' does not exist below assets/"
            )

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
    for region in level.get("floor_regions") or []:
        if region.get("material"):
            yield str(region["material"]), "region floor material"
        if region.get("edge_material"):
            yield str(region["edge_material"]), "region edge material"
    for wall in level.get("walls") or []:
        if wall.get("material"):
            yield str(wall["material"]), "wall material"
        for face, material in (wall.get("faces") or {}).items():
            yield str(material), f"wall {face} face material"
        for opening in wall.get("openings") or []:
            if opening.get("glass"):
                yield str(opening["glass"]).strip(), "opening glass material"
    for patch in level.get("floor_patches") or []:
        if patch.get("material"):
            yield str(patch["material"]), "floor patch material"
    # Generic architectural pieces: every material a piece can name is an
    # ordinary catalog material, checked like any other level reference.
    for ramp in level.get("ramps") or []:
        if ramp.get("material"):
            yield str(ramp["material"]), "ramp material"
        if ramp.get("edge_material"):
            yield str(ramp["edge_material"]), "ramp edge material"
    for stair in level.get("stairs") or []:
        if stair.get("material"):
            yield str(stair["material"]), "staircase tread material"
        if stair.get("riser_material"):
            yield str(stair["riser_material"]), "staircase riser material"
        if stair.get("side_material"):
            yield str(stair["side_material"]), "staircase side material"
    for piece in level.get("half_walls") or []:
        if piece.get("material"):
            yield str(piece["material"]), "half wall material"
        if piece.get("end_material"):
            yield str(piece["end_material"]), "half wall end material"
        if piece.get("cap_material"):
            yield str(piece["cap_material"]), "half wall cap material"
    for piece in level.get("columns") or []:
        if piece.get("material"):
            yield str(piece["material"]), "column material"
        if piece.get("cap_material"):
            yield str(piece["cap_material"]), "column cap material"
    for piece in level.get("archways") or []:
        if piece.get("material"):
            yield str(piece["material"]), "archway material"
        if piece.get("reveal_material"):
            yield str(piece["reveal_material"]), "archway reveal material"
    for rail in level.get("guardrails") or []:
        if rail.get("material"):
            yield str(rail["material"]), "guardrail material"
        if rail.get("post_material"):
            yield str(rail["post_material"]), "guardrail post material"
    for strip in level.get("thresholds") or []:
        if strip.get("material"):
            yield str(strip["material"]), "threshold material"
    for board in level.get("baseboards") or []:
        if board.get("material"):
            yield str(board["material"]), "baseboard material"
    for light in level.get("ceiling_lights") or []:
        if light.get("fixture"):
            yield str(light["fixture"]), "ceiling light fixture"
    for decal in level.get("decals") or []:
        if decal.get("material"):
            yield str(decal["material"]), "decal sheet"
    for prop in level.get("props") or []:
        if prop.get("model"):
            yield str(prop["model"]), "prop"
    for index, animation in enumerate(level.get("animated_emissions") or []):
        if animation.get("material"):
            yield str(animation["material"]).strip(), f"animated_emissions[{index}] material"


def validate_animated_emissions(level: dict, where: str, errors: list[str]) -> None:
    """Animated emissions: a material id, a known effect and bounded rates.

    A malformed animation is an error rather than a silent no-op: a sign that
    was meant to breathe and does not is a bug the author has to see.
    """
    animations = level.get("animated_emissions")
    if animations is None:
        return
    if not isinstance(animations, list):
        errors.append(f"{where}: animated_emissions must be a list")
        return
    for index, animation in enumerate(animations):
        entry_where = f"{where}: animated_emissions[{index}]"
        if not isinstance(animation, dict):
            errors.append(f"{entry_where} must be an object")
            continue
        material = animation.get("material")
        if not isinstance(material, str) or not _ASSET_ID.match(material.strip()):
            errors.append(f"{entry_where}: material must be a well-formed asset id")
        effect = animation.get("effect")
        if effect is not None and str(effect).strip().lower() not in ("pulse", "flicker"):
            errors.append(
                f"{entry_where}: effect must be 'pulse' or 'flicker', found '{effect}'"
            )
        hz = animation.get("hz")
        if hz is not None and (not is_finite_number(hz) or not 0.0 < float(hz) <= 24.0):
            errors.append(f"{entry_where}: hz must be a number above 0 and at most 24")
        if effect is not None and str(effect).strip().lower() == "pulse" and hz is not None:
            if float(hz) > 2.0:
                errors.append(f"{entry_where}: a pulse must be at most 2 Hz")
        depth = animation.get("depth")
        if depth is not None and (not is_finite_number(depth) or not 0.0 < float(depth) <= 0.85):
            errors.append(f"{entry_where}: depth must be a number above 0 and at most 0.85")
        phase = animation.get("phase")
        if phase is not None and not is_finite_number(phase):
            errors.append(f"{entry_where}: phase must be a finite number")


def validate_surface_shine(level: dict, where: str, errors: list[str]) -> None:
    """Every per-surface ``shine`` override must be a unit number.

    Shine is the author-facing glossiness (``0.0`` matte .. ``1.0`` extremely
    glossy); the shipped engine also rejects a malformed value by name, so this
    check keeps the authoring tool and the loader in step.
    """
    defaults = level.get("defaults") or {}
    checks = [(f"{where}: defaults.{key}", defaults.get(key)) for key in ("wall_shine", "floor_shine", "ceiling_shine")]
    rooms = list(level.get("rooms") or [])
    if level.get("room"):
        rooms.append(level["room"])
    for index, room in enumerate(rooms):
        checks.append((f"{where}: room {index} shine", room.get("shine")))
        checks.append((f"{where}: room {index} ceiling_shine", room.get("ceiling_shine")))
    for index, wall in enumerate(level.get("walls") or []):
        checks.append((f"{where}: wall {index} shine", wall.get("shine")))
        for face, shine in (wall.get("face_shine") or {}).items():
            checks.append((f"{where}: wall {index} face '{face}' shine", shine))
        for opening_index, opening in enumerate(wall.get("openings") or []):
            checks.append(
                (f"{where}: wall {index} opening {opening_index} glass_shine", opening.get("glass_shine"))
            )
    for index, patch in enumerate(level.get("floor_patches") or []):
        checks.append((f"{where}: floor patch {index} shine", patch.get("shine")))
    for index, region in enumerate(level.get("floor_regions") or []):
        checks.append((f"{where}: floor region {index} shine", region.get("shine")))
        checks.append((f"{where}: floor region {index} edge_shine", region.get("edge_shine")))
    for index, ramp in enumerate(level.get("ramps") or []):
        checks.append((f"{where}: ramp {index} shine", ramp.get("shine")))
        checks.append((f"{where}: ramp {index} edge_shine", ramp.get("edge_shine")))
    for index, stair in enumerate(level.get("stairs") or []):
        checks.append((f"{where}: staircase {index} shine", stair.get("shine")))
        checks.append((f"{where}: staircase {index} riser_shine", stair.get("riser_shine")))
        checks.append((f"{where}: staircase {index} side_shine", stair.get("side_shine")))
    for index, piece in enumerate(level.get("half_walls") or []):
        checks.append((f"{where}: half wall {index} shine", piece.get("shine")))
        checks.append((f"{where}: half wall {index} end_shine", piece.get("end_shine")))
        checks.append((f"{where}: half wall {index} cap_shine", piece.get("cap_shine")))
    for index, piece in enumerate(level.get("columns") or []):
        checks.append((f"{where}: column {index} shine", piece.get("shine")))
        checks.append((f"{where}: column {index} cap_shine", piece.get("cap_shine")))
    for index, piece in enumerate(level.get("archways") or []):
        checks.append((f"{where}: archway {index} shine", piece.get("shine")))
        checks.append((f"{where}: archway {index} reveal_shine", piece.get("reveal_shine")))
    for index, rail in enumerate(level.get("guardrails") or []):
        checks.append((f"{where}: guardrail {index} shine", rail.get("shine")))
        checks.append((f"{where}: guardrail {index} post_shine", rail.get("post_shine")))
    for index, piece in enumerate(level.get("arc_walls") or []):
        checks.append((f"{where}: arc wall {index} shine", piece.get("shine")))
        checks.append((f"{where}: arc wall {index} inner_shine", piece.get("inner_shine")))
        checks.append((f"{where}: arc wall {index} outer_shine", piece.get("outer_shine")))
        checks.append((f"{where}: arc wall {index} cap_shine", piece.get("cap_shine")))
        checks.append((f"{where}: arc wall {index} end_shine", piece.get("end_shine")))
    for index, piece in enumerate(level.get("pillars") or []):
        checks.append((f"{where}: pillar {index} shine", piece.get("shine")))
        checks.append((f"{where}: pillar {index} cap_shine", piece.get("cap_shine")))
    for index, strip in enumerate(level.get("thresholds") or []):
        checks.append((f"{where}: threshold {index} shine", strip.get("shine")))
    for index, board in enumerate(level.get("baseboards") or []):
        checks.append((f"{where}: baseboard {index} shine", board.get("shine")))
    for label, value in checks:
        if value is None:
            continue
        if not is_finite_number(value) or not 0.0 <= value <= 1.0:
            errors.append(f"{label} must be a number between 0 and 1")


def validate_architecture(level: dict, where: str, errors: list[str]) -> None:
    """Basic shape checks for the generic architectural pieces.

    The loader owns the full contract (slopes, risers, opening geometry); this
    check catches the mistakes an author makes while typing — a missing size, a
    negative one, a non-number — with the piece named.
    """
    positive = ("width", "depth", "length", "height", "rise", "steps", "opening_width",
                "opening_height", "thickness", "radius", "segments")
    pieces = (
        ("ramps", "ramp", ("width", "depth", "rise")),
        ("stairs", "staircase", ("width", "depth", "rise", "steps")),
        ("half_walls", "half wall", ("width", "depth", "height")),
        ("columns", "column", ("width", "depth")),
        ("arc_walls", "arc wall", ("radius",)),
        ("pillars", "pillar", ("radius",)),
        ("archways", "archway", ("width", "depth", "height", "opening_width", "opening_height")),
        ("guardrails", "guardrail", ("length",)),
        ("thresholds", "threshold", ("length",)),
        ("baseboards", "baseboard", ("length",)),
    )
    for key, label, required in pieces:
        entries = level.get(key)
        if entries is None:
            continue
        if not isinstance(entries, list):
            errors.append(f"{where}: {key} must be a list")
            continue
        for index, piece in enumerate(entries):
            entry_where = f"{where}: {key}[{index}]"
            if not isinstance(piece, dict):
                errors.append(f"{entry_where} must be an object")
                continue
            for field in required:
                value = piece.get(field)
                if value is None:
                    errors.append(f"{entry_where}: {field} is required")
                    continue
                if not is_finite_number(value):
                    errors.append(f"{entry_where}: {field} must be a finite number")
                    continue
                if float(value) <= 0.0:
                    errors.append(f"{entry_where}: {field} must be positive")
            for field in positive:
                value = piece.get(field)
                if value is not None and is_finite_number(value) and float(value) < 0.0:
                    errors.append(f"{entry_where}: {field} cannot be negative")
            if key == "arc_walls":
                thickness = piece.get("thickness")
                radius = piece.get("radius")
                if (
                    is_finite_number(thickness)
                    and is_finite_number(radius)
                    and float(thickness) >= 2.0 * float(radius)
                ):
                    errors.append(
                        f"{entry_where}: thickness must be thinner than twice the radius"
                    )
                segments = piece.get("segments")
                if segments is not None and (
                    not isinstance(segments, int) or not 3 <= segments <= 128
                ):
                    errors.append(f"{entry_where}: segments must be an integer between 3 and 128")
            if key == "pillars":
                segments = piece.get("segments")
                if segments is not None and (
                    not isinstance(segments, int) or not 3 <= segments <= 128
                ):
                    errors.append(f"{entry_where}: segments must be an integer between 3 and 128")


def wall_touches_any_room(level: dict, wall: dict, epsilon: float = 0.05) -> bool:
    """True when a wall's footprint meets at least one room's footprint.

    Walls are placed by their **minimum corner** (like rooms), so a wall
    authored by its centre usually sits entirely outside its room and leaves
    the shell open — the void then renders as a black hole in game. This is a
    warning, not an error: freestanding walls are legal level content.
    """
    try:
        wx = float(wall.get("x", 0.0))
        wz = float(wall.get("z", 0.0))
        ww = float(wall.get("width", 0.0))
        wd = float(wall.get("depth", 0.0))
    except (TypeError, ValueError):
        return True
    if ww <= 0.0 or wd <= 0.0:
        return True
    x0, x1 = wx - epsilon, wx + ww + epsilon
    z0, z1 = wz - epsilon, wz + wd + epsilon
    rooms = list(level.get("rooms") or [])
    if level.get("room"):
        rooms.append(level["room"])
    for room in rooms:
        try:
            rx = float(room.get("x", 0.0))
            rz = float(room.get("z", 0.0))
            rw = float(room.get("width", 0.0))
            rd = float(room.get("depth", 0.0))
        except (TypeError, ValueError):
            continue
        if x0 <= rx + rw and rx <= x1 and z0 <= rz + rd and rz <= z1:
            return True
    return False


def _resolve_prop_ids(level: dict) -> List[str]:
    """The engine's per-instance prop ids: authored wins, else `<model>_<n>`.

    Mirrors ``LevelDef::prop_instance_ids``: the counter counts placements that
    do not author an id, per model short name, in array order.
    """
    counters: dict = {}
    ids: List[str] = []
    for prop in level.get("props") or []:
        authored = prop.get("id")
        if isinstance(authored, str) and authored.strip():
            ids.append(authored.strip())
            continue
        short = str(prop.get("model", "")).rsplit(":", 1)[-1] or "prop"
        counters[short] = counters.get(short, 0) + 1
        ids.append(f"{short}_{counters[short]}")
    return ids


def validate_interactions(level: dict, where: str, errors: List[str]) -> None:
    """Instance ids, prop interactions, area triggers and entity routes.

    Mirrors the Rust loader's rules so a level that passes this validator
    cannot surprise the engine: well-formed unique ids, 1..8 typed actions with
    resolvable targets, implemented actions only, and real trigger volumes. The
    reserved ``play_audio`` tag is rejected by name rather than silently
    ignored; ``play_animation`` is implemented and checked like ``toggle_label``.
    """
    prop_ids = _resolve_prop_ids(level)
    seen: dict = {}
    for index, prop_id in enumerate(prop_ids):
        if not _ASSET_ID.match(prop_id):
            errors.append(f"{where}: prop {index} id '{prop_id}' must be a well-formed identifier")
        elif prop_id in seen:
            errors.append(f"{where}: prop {index} id '{prop_id}' duplicates {seen[prop_id]}")
        else:
            seen[prop_id] = f"prop {index}"
    for index, prop in enumerate(level.get("props") or []):
        name = prop.get("display_name")
        if name is not None and (not isinstance(name, str) or not name.strip()):
            errors.append(f"{where}: prop {index} display_name must not be blank")
    trigger_ids = []
    for index, trigger in enumerate(level.get("area_triggers") or []):
        authored = trigger.get("id")
        trigger_id = (
            authored.strip()
            if isinstance(authored, str) and authored.strip()
            else f"trigger_{index + 1}"
        )
        trigger_ids.append(trigger_id)
        if not _ASSET_ID.match(trigger_id):
            errors.append(f"{where}: area trigger {index} id '{trigger_id}' must be a well-formed identifier")
        elif trigger_id in seen:
            errors.append(f"{where}: area trigger {index} id '{trigger_id}' duplicates {seen[trigger_id]}")
        else:
            seen[trigger_id] = f"area trigger {index}"
    for index, fixture in enumerate(level.get("ceiling_lights") or []):
        authored = fixture.get("id")
        if authored is None:
            continue
        fixture_id = str(authored).strip()
        if not _ASSET_ID.match(fixture_id):
            errors.append(f"{where}: ceiling light {index} id '{fixture_id}' must be a well-formed identifier")
        elif fixture_id in seen:
            errors.append(f"{where}: ceiling light {index} id '{fixture_id}' duplicates {seen[fixture_id]}")
        else:
            seen[fixture_id] = f"ceiling light {index}"

    actions_by_source = [
        (f"prop {index} interaction", prop.get("interaction"), prop_ids[index])
        for index, prop in enumerate(level.get("props") or [])
        if prop.get("interaction") is not None
    ]
    actions_by_source.extend(
        (f"area trigger {index}", trigger, None)
        for index, trigger in enumerate(level.get("area_triggers") or [])
    )
    for source, definition, implicit_target in actions_by_source:
        if not isinstance(definition, dict):
            errors.append(f"{where}: {source} must be an object")
            continue
        reach = definition.get("reach")
        if reach is not None and (
            not is_finite_number(reach) or reach <= 0.0 or reach > MAX_INTERACTION_REACH_M
        ):
            errors.append(f"{where}: {source} reach must be a finite number in (0, {MAX_INTERACTION_REACH_M}]")
        actions = definition.get("actions")
        if not isinstance(actions, list) or not actions:
            errors.append(f"{where}: {source} must declare 1..{MAX_ACTIONS_PER_SOURCE} actions")
            continue
        if len(actions) > MAX_ACTIONS_PER_SOURCE:
            errors.append(f"{where}: {source} declares {len(actions)} actions; the limit is {MAX_ACTIONS_PER_SOURCE}")
        for action_index, action in enumerate(actions):
            if not isinstance(action, dict):
                errors.append(f"{where}: {source} action {action_index} must be an object")
                continue
            tag = action.get("action")
            if tag not in ACTION_TAGS:
                errors.append(
                    f"{where}: {source} action {action_index} has unknown action '{tag}'"
                )
            elif tag not in IMPLEMENTED_ACTIONS:
                errors.append(
                    f"{where}: {source} action {action_index} ('{tag}') is not implemented yet"
                )
            elif tag == "toggle_label":
                target = action.get("target")
                resolved = target.strip() if isinstance(target, str) and target.strip() else implicit_target
                if not resolved:
                    errors.append(f"{where}: {source} action {action_index} ('toggle_label') needs a target")
                elif resolved not in prop_ids:
                    errors.append(
                        f"{where}: {source} action {action_index} ('toggle_label') targets unknown instance '{resolved}'"
                    )
            elif tag == "play_animation":
                target = action.get("target")
                if isinstance(target, str) and not target.strip():
                    errors.append(
                        f"{where}: {source} action {action_index} ('play_animation') target must not be blank"
                    )
                resolved = target.strip() if isinstance(target, str) and target.strip() else implicit_target
                if not resolved:
                    errors.append(
                        f"{where}: {source} action {action_index} ('play_animation') needs a target"
                    )
                elif resolved not in prop_ids:
                    errors.append(
                        f"{where}: {source} action {action_index} ('play_animation') targets unknown instance '{resolved}'"
                    )
                clip = action.get("clip")
                if not isinstance(clip, str) or not clip.strip():
                    errors.append(
                        f"{where}: {source} action {action_index} ('play_animation') needs a clip name"
                    )
                looped = action.get("loop", False)
                if not isinstance(looped, bool):
                    errors.append(
                        f"{where}: {source} action {action_index} ('play_animation') loop must be a boolean"
                    )
            elif tag == "toggle_animation":
                target = action.get("target")
                if isinstance(target, str) and not target.strip():
                    errors.append(
                        f"{where}: {source} action {action_index} ('toggle_animation') target must not be blank"
                    )
                resolved = target.strip() if isinstance(target, str) and target.strip() else implicit_target
                if not resolved:
                    errors.append(
                        f"{where}: {source} action {action_index} ('toggle_animation') needs a target"
                    )
                elif resolved not in prop_ids:
                    errors.append(
                        f"{where}: {source} action {action_index} ('toggle_animation') targets unknown instance '{resolved}'"
                    )
                clip = action.get("clip")
                if not isinstance(clip, str) or not clip.strip():
                    errors.append(
                        f"{where}: {source} action {action_index} ('toggle_animation') needs a clip name"
                    )

    triggers = level.get("area_triggers") or []
    if isinstance(triggers, list) and len(triggers) > MAX_AREA_TRIGGERS:
        errors.append(f"{where}: too many area triggers ({len(triggers)}; limit {MAX_AREA_TRIGGERS})")
    for index, trigger in enumerate(triggers if isinstance(triggers, list) else []):
        if not isinstance(trigger, dict):
            errors.append(f"{where}: area trigger {index} must be an object")
            continue
        width = trigger.get("width")
        depth = trigger.get("depth")
        if not is_finite_number(width) or not is_finite_number(depth) or width <= 0.0 or depth <= 0.0:
            errors.append(f"{where}: area trigger {index} width and depth must be positive numbers")
        cooldown = trigger.get("cooldown_seconds", 0.0)
        if not is_finite_number(cooldown) or cooldown < 0.0:
            errors.append(f"{where}: area trigger {index} cooldown_seconds must be a finite number >= 0")
        bottom = trigger.get("bottom_y")
        top = trigger.get("top_y")
        if bottom is not None and not is_finite_number(bottom):
            errors.append(f"{where}: area trigger {index} bottom_y must be a finite number")
        if top is not None and not is_finite_number(top):
            errors.append(f"{where}: area trigger {index} top_y must be a finite number")
        if is_finite_number(bottom) and is_finite_number(top) and top <= bottom:
            errors.append(f"{where}: area trigger {index} top_y must be above its bottom_y")
        if is_finite_number(width) and is_finite_number(depth) and width > 0.0 and depth > 0.0:
            footprint = {
                "x": trigger.get("x", 0.0),
                "z": trigger.get("z", 0.0),
                "width": width,
                "depth": depth,
            }
            if not wall_touches_any_room(level, footprint):
                errors.append(f"{where}: area trigger {index} lies outside every room section")

    validate_entity_routes(level, where, errors, prop_ids)


def validate_floats(level: dict, where: str, errors: List[str]) -> None:
    """Mirrors ``loader::validate_floats``: a floating prop cannot be solid, its
    authored motion must be finite and bounded, it cannot be routed, and its
    whole swept footprint must sit inside one water volume.

    The containment rule is what keeps a float off the rim: the half-diagonal
    of the authored footprint plus the heel's horizontal excursion must fit
    inside the basin rectangle, so no phase or frame can push the hull through
    the skirt.
    """
    props = level.get("props") or []
    if not isinstance(props, list):
        return
    floats = [
        (index, prop)
        for index, prop in enumerate(props)
        if isinstance(prop, dict) and prop.get("float") is not None
    ]
    if not floats:
        return
    if len(floats) > MAX_LEVEL_FLOAT_PROPS:
        errors.append(
            f"{where}: {len(floats)} floating props; the limit is {MAX_LEVEL_FLOAT_PROPS}"
        )
    volumes = []
    for volume in level.get("water") or []:
        if not isinstance(volume, dict):
            continue
        x, z = volume.get("x", 0.0), volume.get("z", 0.0)
        width, depth = volume.get("width"), volume.get("depth")
        if (
            is_finite_number(x)
            and is_finite_number(z)
            and is_finite_number(width)
            and is_finite_number(depth)
            and width > 0.0
            and depth > 0.0
        ):
            volumes.append(
                (min(x, x + width), max(x, x + width), min(z, z + depth), max(z, z + depth))
            )
    route_ids = {
        str(route.get("id")).strip()
        for route in (level.get("routes") or [])
        if isinstance(route, dict) and route.get("id")
    }
    prop_ids = _resolve_prop_ids(level)
    for index, prop in floats:
        context = f"{where}: float prop {index} ('{prop.get('model')}')"
        if prop.get("solid") is True:
            errors.append(
                f"{context} must set solid: false; a floating hull cannot leave a static collider"
            )
        definition = prop.get("float")
        if not isinstance(definition, dict):
            errors.append(f"{context} float must be an object")
            continue
        x, z = prop.get("x", 0.0), prop.get("z", 0.0)
        scale = prop.get("scale", 1.0)
        size = prop.get("size")
        if not is_finite_number(x) or not is_finite_number(z) or not is_finite_number(scale) or scale <= 0.0:
            errors.append(f"{context} position and scale must be finite and its scale positive")
            continue
        if not isinstance(size, list) or len(size) != 3 or not all(
            is_finite_number(value) and value > 0.0 for value in size
        ):
            errors.append(
                f"{context} must author size: the float contract is validated against its footprint"
            )
            continue
        draft = definition.get("draft", 0.0)
        bob = definition.get("bob", 0.0)
        heel = definition.get("heel_degrees", 0.0)
        bob_seconds = definition.get("bob_seconds", 2.4)
        heel_seconds = definition.get("heel_seconds", 2.4)
        phase = definition.get("phase")
        height = size[1] * scale
        width = size[0] * scale
        depth = size[2] * scale
        if not is_finite_number(draft) or draft <= 0.0 or draft >= height:
            errors.append(
                f"{context} draft must be finite, above 0 and below its height ({height:.3f} m)"
            )
        if not is_finite_number(bob) or bob < 0.0 or bob > 0.5 * height:
            errors.append(f"{context} bob must be finite, >= 0 and at most half its height")
        if not is_finite_number(bob_seconds) or bob_seconds <= 0.0:
            errors.append(f"{context} bob_seconds must be a positive finite number")
        if (
            not is_finite_number(heel)
            or heel < 0.0
            or heel > MAX_FLOAT_HEEL_DEGREES
        ):
            errors.append(
                f"{context} heel_degrees must be finite, >= 0 and at most {MAX_FLOAT_HEEL_DEGREES}"
            )
        if not is_finite_number(heel_seconds) or heel_seconds <= 0.0:
            errors.append(f"{context} heel_seconds must be a positive finite number")
        if phase is not None and (not is_finite_number(phase) or not 0.0 <= phase <= 1.0):
            errors.append(f"{context} phase must be a finite number between 0.0 and 1.0")
        instance_id = prop_ids[index] if index < len(prop_ids) else None
        if instance_id and instance_id in route_ids:
            errors.append(f"{context} is addressed by a route; a floating prop cannot be routed")
        if not all(is_finite_number(value) for value in (draft, bob, heel)):
            continue
        half_diagonal = 0.5 * math.hypot(width, depth)
        heel_excursion = 0.5 * height * math.sin(math.radians(heel))
        radius = half_diagonal + heel_excursion
        contained = any(
            x - radius >= x0 and x + radius <= x1 and z - radius >= z0 and z + radius <= z1
            for (x0, x1, z0, z1) in volumes
        )
        if not contained:
            errors.append(
                f"{context} must be fully inside a water volume: its swept footprint "
                f"(radius {radius:.3f} m) is not contained at ({x}, {z})"
            )


def validate_entity_routes(level: dict, where: str, errors: List[str], prop_ids: List[str]) -> None:
    """Authored entity routes: resolving unique ids and bounded, typed steps.

    Mirrors the Rust loader's structural rules (``loader::validate_routes``):
    a route must name a placed instance, carry 1..MAX_ROUTE_STEPS known step
    kinds with finite bounded numbers and a non-blank clip name. Floor and wall
    geometry along the path is checked only by the Rust loader, which samples
    the real walkable surface; this validator keeps the schema honest so a
    malformed map never reaches the engine.
    """
    routes = level.get("routes") or []
    if not isinstance(routes, list):
        errors.append(f"{where}: routes must be an array")
        return
    if len(routes) > MAX_ENTITY_ROUTES:
        errors.append(f"{where}: too many entity routes ({len(routes)}; limit {MAX_ENTITY_ROUTES})")
    seen_routes: set[str] = set()
    for index, route in enumerate(routes):
        if not isinstance(route, dict):
            errors.append(f"{where}: entity route {index} must be an object")
            continue
        route_id = route.get("id")
        if not isinstance(route_id, str) or not route_id.strip():
            errors.append(f"{where}: entity route {index} names no instance id")
            continue
        route_id = route_id.strip()
        if route_id in seen_routes:
            errors.append(f"{where}: entity route {index} duplicates instance '{route_id}'")
        seen_routes.add(route_id)
        if route_id not in prop_ids:
            errors.append(f"{where}: entity route {index} targets unknown instance '{route_id}'")
        looped = route.get("loop", False)
        if not isinstance(looped, bool):
            errors.append(f"{where}: entity route {index} loop must be a boolean")
        steps = route.get("steps")
        if not isinstance(steps, list) or not steps:
            errors.append(f"{where}: entity route '{route_id}' declares no steps")
            continue
        if len(steps) > MAX_ROUTE_STEPS:
            errors.append(
                f"{where}: entity route '{route_id}' declares {len(steps)} steps; limit {MAX_ROUTE_STEPS}"
            )
        for step_index, step in enumerate(steps):
            source = f"entity route '{route_id}' step {step_index}"
            if not isinstance(step, dict):
                errors.append(f"{where}: {source} must be an object")
                continue
            tag = step.get("step")
            if tag not in ROUTE_STEP_TAGS:
                errors.append(f"{where}: {source} has unknown step '{tag}'")
                continue
            if tag == "move_to":
                for key in ("x", "z", "speed"):
                    if not is_finite_number(step.get(key)):
                        errors.append(f"{where}: {source} ('move_to') {key} must be a finite number")
                speed = step.get("speed")
                if is_finite_number(speed) and not 0.0 < speed <= MAX_ROUTE_SPEED_MPS:
                    errors.append(
                        f"{where}: {source} ('move_to') speed must be in (0, {MAX_ROUTE_SPEED_MPS}]"
                    )
            elif tag == "face":
                if not is_finite_number(step.get("yaw_degrees")):
                    errors.append(f"{where}: {source} ('face') yaw_degrees must be a finite number")
            elif tag == "wait":
                seconds = step.get("seconds")
                if not is_finite_number(seconds) or not 0.0 < seconds <= MAX_ROUTE_SECONDS:
                    errors.append(f"{where}: {source} ('wait') seconds must be in (0, {MAX_ROUTE_SECONDS}]")
            elif tag == "play":
                clip = step.get("clip")
                if not isinstance(clip, str) or not clip.strip():
                    errors.append(f"{where}: {source} ('play') needs a clip name")
                seconds = step.get("seconds")
                if not is_finite_number(seconds) or not 0.0 < seconds <= MAX_ROUTE_SECONDS:
                    errors.append(f"{where}: {source} ('play') seconds must be in (0, {MAX_ROUTE_SECONDS}]")
                looped = step.get("loop", False)
                if not isinstance(looped, bool):
                    errors.append(f"{where}: {source} ('play') loop must be a boolean")


def validate_levels(catalog: dict, level_dirs: Tuple[str, ...] = LEVEL_DIRS) -> Tuple[List[str], List[str]]:
    """Returns ``(errors, warnings)`` for every shipped level, drop-in level and fixture.

    ``level_dirs`` defaults to the shipped/drop-in/fixture directories; tests
    pass one temporary directory to validate a single authored level document.
    """
    errors: List[str] = []
    warnings: List[str] = []
    known = {str(entry.get("id")) for entry in catalog_entries(catalog)}
    placeable = {str(entry.get("id")) for entry in placeable_entries(catalog)}
    levels = 0
    for directory in level_dirs:
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
            relative = os.path.relpath(path, PACKAGE_ROOT)
            # Optional schema: a fixture may be switched off without losing its
            # visible glow, and a prop may own generic light sources. Both are
            # validated with the same rules the engine enforces.
            for index, light in enumerate(level.get("ceiling_lights") or []):
                where = f"{relative}: ceiling light {index}"
                if "enabled" in light and not isinstance(light.get("enabled"), bool):
                    errors.append(f"{where} enabled must be a boolean")
                fixture_range = light.get("range")
                if fixture_range is not None and (
                    not is_finite_number(fixture_range) or fixture_range <= 0.0
                ):
                    errors.append(f"{where} range must be a finite number > 0")
                falloff = light.get("falloff")
                if falloff is not None and (
                    not isinstance(falloff, str) or falloff not in LIGHT_FALLOFFS
                ):
                    errors.append(f"{where} falloff must be one of {', '.join(LIGHT_FALLOFFS)}")
                emission = light.get("emission")
                if emission is not None and (
                    not is_finite_number(emission) or emission < 0.0
                ):
                    errors.append(f"{where} emission must be a finite number >= 0")
            for index, prop in enumerate(level.get("props") or []):
                lights = prop.get("lights")
                if lights is None:
                    continue
                if not isinstance(lights, list):
                    errors.append(f"{relative}: prop {index} lights must be an array")
                    continue
                for light_index, light in enumerate(lights):
                    light_errors, light_warnings = validate_light_source(
                        light, f"{relative}: prop {index} light {light_index}"
                    )
                    errors.extend(light_errors)
                    warnings.extend(light_warnings)
            validate_animated_emissions(level, relative, errors)
            validate_interactions(level, relative, errors)
            validate_floats(level, relative, errors)
            validate_surface_shine(level, relative, errors)
            validate_architecture(level, relative, errors)
            rooms = list(level.get("rooms") or [])
            if level.get("room"):
                rooms.append(level["room"])
            if rooms:
                for index, wall in enumerate(level.get("walls") or []):
                    if wall_touches_any_room(level, wall):
                        continue
                    warnings.append(
                        f"{relative}: wall {index} at ({wall.get('x')}, {wall.get('z')}) "
                        f"{wall.get('width')}x{wall.get('depth')} touches no room; walls are placed by their "
                        "minimum corner, so a centre-authored wall usually leaves the shell open"
                    )
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

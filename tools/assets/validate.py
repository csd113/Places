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

        # Batch 3 surface response and alpha. Like emission these are material
        # definition fields: a prop or a texture that authored them would be a
        # silent no-op, so it is an error.
        normal_texture = entry.get("normal_texture")
        normal_strength = entry.get("normal_strength")
        specular = entry.get("specular")
        specular_color = entry.get("specular_color")
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
            if roughness is not None and (
                not is_finite_number(roughness) or not 0.0 <= roughness <= 1.0
            ):
                errors.append(f"{where}: roughness must be a number between 0 and 1")
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
            # Batch 4 selective reflections: a mode plus its weight.
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
    """Batch 4 animated emissions: a material id, a known effect and bounded rates.

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
            # validated with the same rules the editor and the engine enforce.
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

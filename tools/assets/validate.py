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
  ``props[].lights`` sources obey the documented shapes, dimensions and ranges;
* the v3 entity layer: instance ids and typed components, event bindings,
  conditions and actions with resolvable capability-fit targets, trigger
  volumes, timers, sequences and spawn templates/points/groups.

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

# Generated capacity fixtures deliberately reference synthetic material ids.
# The catalog-id cross-check exists to catch a typo in authored content; a
# machine-generated fixture whose purpose is to cross the former 16-bit
# material-index boundary with more than 65 536 distinct ids cannot use the
# catalog at all. The fixture's real contracts are pinned by src/zoo_audit.rs,
# and every other check in this validator still applies to it. Mirror of the
# same carve-out in src/assets/tests.rs.
GENERATED_CAPACITY_FIXTURES = ("capacity_beyond_former_limits.json",)

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
# implements. The reserved/unimplemented tags are rejected by name, not
# silently ignored. Audio (`play_sound`/`stop_sound`) has no subsystem yet, so
# it is parsed but unimplemented; there is no `play_audio` tag at all.
ACTION_TAGS = (
    "open",
    "close",
    "toggle",
    "enable",
    "disable",
    "set_light",
    "lock",
    "unlock",
    "play_animation",
    "toggle_animation",
    "play_sound",
    "stop_sound",
    "change_material",
    "move_object",
    "set_state",
    "toggle_label",
    "start_sequence",
    "stop_sequence",
    "start_timer",
    "stop_timer",
    "spawn_entity",
    "despawn_entity",
    "reset_to_start",
)
IMPLEMENTED_ACTIONS = tuple(
    tag for tag in ACTION_TAGS if tag not in ("play_sound", "stop_sound")
)

# Typed components, event kinds and condition checks the runtime defines
# (src/level.rs).
COMPONENT_TAGS = (
    "interactable",
    "animation",
    "audio",
    "light",
    "material",
    "state",
    "lifetime",
    "steam",
    "water",
    "nav_agent",
    "nav_obstacle",
    "ai",
    "fade",
    "glow",
)
EVENT_KINDS = (
    "interact",
    "enter_volume",
    "exit_volume",
    "timer",
    "object_state",
    "sequence_complete",
    "spawn",
    "animation_complete",
    "ai_state",
    "caught",
)
CONDITION_CHECKS = (
    "state",
    "enabled",
    "disabled",
    "locked",
    "unlocked",
    "door_open",
    "door_closed",
    "sequence_running",
    "sequence_idle",
)
SEQUENCE_STEP_TAGS = (
    "action",
    "wait",
    "move",
    "face",
    "wait_animation",
    "emit",
    "set_state",
    "stop",
)

# Mirrors src/level.rs: a float's motion is bounded and must be contained by
# the water volume it rides.
MAX_LEVEL_FLOAT_PROPS = 32
MAX_FLOAT_HEEL_DEGREES = 45.0
# Mirrors the v3 schema's counted resources (src/level.rs and
# src/entities/{sequences,spawn}.rs); reach mirrors src/interact.rs.
MAX_ACTIONS_PER_SOURCE = 8
MAX_BINDINGS_PER_ENTITY = 16
MAX_AREA_TRIGGERS = 1000
MAX_INTERACTION_REACH_M = 4.0
# Mirrors src/level.rs: fade period and glow intensity/range/offset bounds.
MAX_FADE_PERIOD_SECONDS = 3600.0
MAX_FADE_SECONDS = 600.0
DEFAULT_FADE_PERIOD_SECONDS = 6.0
MAX_GLOW_INTENSITY = 8.0
MIN_GLOW_RANGE_M = 0.05
MAX_GLOW_RANGE_M = 64.0
MAX_GLOW_OFFSET_M = 4.0
# Mirrors src/level.rs: the water volume budget and the circle shape code.
MAX_LEVEL_WATER_VOLUMES = 8000
WATER_SHAPES = ("rect", "circle")
MAX_ENTITY_ROUTES = 256
MAX_LEVEL_SEQUENCES = 256
MAX_SEQUENCE_STEPS = 64
MAX_SEQUENCE_WAIT_S = 600.0
MAX_SEQUENCE_ANIMATION_TIMEOUT_S = 120.0
MAX_LEVEL_SPAWN_TEMPLATES = 64
MAX_LEVEL_SPAWN_POINTS = 256
MAX_LEVEL_SPAWN_GROUPS = 64
# Mirrors src/level.rs: two dynamic objects per door against the renderer's
# MAX_DYNAMIC_OBJECTS budget, leaving room for floats and demonstration objects.
MAX_LEVEL_DOORS = 24
DEFAULT_STEAM_MATERIAL = "core:steam_01"
MAX_LEVEL_EFFECTS = 64
MAX_EFFECT_PARTICLES = 128
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
    """Every entry of the catalog's single ``assets`` array."""
    return list(catalog.get("assets", []))


def placeable_entries(catalog: dict) -> List[dict]:
    return [
        entry
        for entry in catalog_entries(catalog)
        if entry.get("asset_type") in PLACEABLE_TYPES
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
        if not str(theme.get("display_name") or "").strip():
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
        if surface is not None and str(surface).strip() not in ("wall", "floor", "ceiling", "sky"):
            errors.append(f"{where}: surface must be 'wall', 'floor', 'ceiling' or 'sky'")

        # Surface-response and alpha fields. Like emission these are material
        # definition fields: a prop or a texture that authored them would be a
        # silent no-op, so it is an error.
        normal_texture = entry.get("normal_texture")
        normal_strength = entry.get("normal_strength")
        specular = entry.get("specular")
        specular_color = entry.get("specular_color")
        shine = entry.get("shine")
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
                "alpha_mode",
                "opacity",
                "alpha_cutoff",
                "reflection_mode",
                "reflection_strength",
            )
        )
        if declares_response and not (asset_type == "material" and source == "definition"):
            # A file-backed decal sheet may author only its alpha handling: a
            # soft-edged `blend` sheet is the path-to-grass feather, and it has
            # no other material state to declare.
            decal_alpha_only = (
                asset_type == "decal"
                and source == "file"
                and normal_texture is None
                and normal_strength is None
                and specular is None
                and specular_color is None
                and shine is None
                and opacity is None
                and alpha_cutoff is None
                and reflection_mode is None
                and reflection_strength is None
            )
            if not decal_alpha_only:
                errors.append(
                    f"{where}: surface-response and alpha fields are only valid on a definition "
                    f"material; a file decal may declare only alpha_mode"
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
                errors.append("spooner-man: a duplicate copy exists at assets/props/models/spooner-man.glb")
        if spooner.get("theme") is not None:
            errors.append("spooner-man: an entity must not carry an environment theme")

    return errors, warnings


def level_ids(level: dict):
    """Yields ``(id, what)`` for every asset reference in a parsed level."""
    sky = level.get("sky")
    if isinstance(sky, dict) and sky.get("texture"):
        yield str(sky["texture"]).strip(), "sky texture"
    defaults = level.get("defaults") or {}
    for key in ("wall", "floor", "ceiling"):
        if defaults.get(key):
            yield str(defaults[key]), f"defaults.{key}"
    rooms = list(level.get("rooms") or [])
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
    for index, door in enumerate(level.get("doors") or []):
        kind = str(door.get("kind", "interior"))
        defaults = {
            "interior": ("home:door_white_01", "home:baseboard_white_01", "core:metal_brass_01", None),
            "sauna": ("home:sauna_wood_01", "home:baseboard_wood_01", "home:sauna_wood_01", "core:glass_window_clear_01"),
        }.get(kind)
        if defaults is None:
            continue
        for key, default in zip(("material", "frame_material", "handle_material", "glass"), defaults):
            if key == "glass" and default is None:
                continue
            author = door.get(key)
            if isinstance(author, str) and author.strip():
                yield author.strip(), f"door {index} {key}"
            elif default is not None:
                yield default, f"door {index} default {key}"
    for index, effect in enumerate(level.get("effects") or []):
        material = effect.get("material")
        yield (
            str(material).strip() if isinstance(material, str) and material.strip() else DEFAULT_STEAM_MATERIAL,
            f"effects[{index}] material",
        )

    weather = level.get("weather")
    if isinstance(weather, dict):
        yield str(weather.get("material", "core:snowflake_01")).strip(), "weather material"


def validate_doors(level: dict, where: str, errors: list[str]) -> None:
    """Doors: count, finite geometry, positive dimensions and a legal kind.

    Mirrors the Rust loader's `validate_doors` field rules; the closed-leaf
    solid test is only meaningful with the collision geometry, which this
    tooling mirror does not build.
    """
    doors = level.get("doors")
    if doors is None:
        return
    if not isinstance(doors, list):
        errors.append(f"{where}: doors must be an array")
        return
    if len(doors) > MAX_LEVEL_DOORS:
        errors.append(f"{where}: too many doors ({len(doors)}; limit {MAX_LEVEL_DOORS})")
    for index, door in enumerate(doors):
        if not isinstance(door, dict):
            errors.append(f"{where}: door {index} must be an object")
            continue
        for key in ("x", "y", "z", "rotation_degrees", "width", "height", "thickness",
                    "swing_degrees", "open_speed_degrees"):
            value = door.get(key)
            if value is not None and not is_finite_number(value):
                errors.append(f"{where}: door {index} {key} must be a finite number")
        width = door.get("width")
        height = door.get("height")
        thickness = door.get("thickness", 0.045)
        if not (is_finite_number(width) and width > 0.0 and is_finite_number(height) and height > 0.0):
            errors.append(f"{where}: door {index} width and height must be positive numbers")
        if is_finite_number(thickness) and thickness <= 0.0:
            errors.append(f"{where}: door {index} thickness must be positive")
        kind = door.get("kind", "interior")
        if kind not in ("interior", "sauna"):
            errors.append(f"{where}: door {index} kind must be 'interior' or 'sauna'")
        direction = door.get("open_direction", "left")
        if direction not in ("left", "right"):
            errors.append(f"{where}: door {index} open_direction must be 'left' or 'right'")
        state = door.get("initial_state", "closed")
        if state not in ("closed", "open"):
            errors.append(f"{where}: door {index} initial_state must be 'closed' or 'open'")
        obstruction = door.get("obstruction", "stop")
        if obstruction not in ("stop", "reverse"):
            errors.append(f"{where}: door {index} obstruction must be 'stop' or 'reverse'")


def validate_environment(level: dict, where: str, errors: list[str]) -> None:
    """Mirror environment.rs defaults, strict field names and authored bounds."""
    environment = level.get("environment")
    if environment is None:
        return
    if not isinstance(environment, dict):
        errors.append(f"{where}: environment must be an object")
        return
    if environment.keys() - {"presentation", "fog"}:
        errors.append(f"{where}: unknown environment fields: {sorted(environment.keys() - {'presentation', 'fog'})}")
    sections = (
        ("presentation", (("exposure", 1.0, .125, 8.0),
                          ("tone_knee", .75, .25, .95),
                          ("saturation", 1.03, .8, 1.2),
                          ("contrast", 1.02, .8, 1.2))),
        ("fog", (("density", .0095, 0.0, .5),
                 ("reference_y", 2.0, -10_000.0, 10_000.0),
                 ("height_gain", .045, 0.0, 1.0))),
    )
    for name, fields in sections:
        section = environment.get(name, {})
        if not isinstance(section, dict):
            errors.append(f"{where}: environment {name} must be an object")
            continue
        allowed = {key for key, _, _, _ in fields}
        if name == "fog":
            allowed.add("color")
        if section.keys() - allowed:
            errors.append(f"{where}: unknown environment {name} fields: {sorted(section.keys() - allowed)}")
        for key, default, low, high in fields:
            value = section.get(key, default)
            if not is_finite_number(value) or not low <= value <= high:
                errors.append(f"{where}: environment {name} {key} must be finite in {low}..{high}")
        if name == "fog" and not is_color_triplet(section.get("color", [.60, .63, .68])):
            errors.append(f"{where}: environment fog color must have three finite sRGB channels in 0..1")


def validate_sky(level: dict, where: str, errors: list[str]) -> None:
    """SkyDef keeps permissive field names and bounded linear dome radiance."""
    sky = level.get("sky")
    if sky is None:
        return
    if not isinstance(sky, dict):
        errors.append(f"{where}: sky must be an object")
        return
    texture = sky.get("texture")
    if not isinstance(texture, str) or not _ASSET_ID.fullmatch(texture.strip()):
        errors.append(f"{where}: sky texture must be a well-formed logical id")
    for key, default, high in (("brightness", 1.0, 4.0), ("ambient", 0.0, 1.0)):
        value = sky.get(key, default)
        if not is_finite_number(value) or not 0.0 <= value <= high:
            errors.append(f"{where}: sky {key} must be finite in 0..{high}")
    color = sky.get("ambient_color")
    if color is not None and not is_color_triplet(color):
        errors.append(f"{where}: sky ambient_color must have three finite linear channels in 0..1")


def validate_weather(level: dict, where: str, errors: list[str]) -> None:
    """Mirror the engine's bounded opt-in snow configuration."""
    weather = level.get("weather")
    if weather is None:
        return
    if not isinstance(weather, dict) or weather.get("kind") != "snow":
        errors.append(f"{where}: weather must be an object with kind 'snow'")
        return
    allowed = {"kind", "count", "radius", "height", "wind", "size", "speed", "opacity", "material", "intensity", "storm_severity", "visibility_m", "fog_color"}
    if weather.keys() - allowed:
        errors.append(f"{where}: unknown weather fields: {sorted(weather.keys() - allowed)}")
    count = weather.get("count", 1400)
    if isinstance(count, bool) or not isinstance(count, int) or not 1 <= count <= 2048:
        errors.append(f"{where}: weather count must be 1..2048")
    for key, default, low, high in (("radius", 16, 4, 32), ("height", 12, 4, 24), ("opacity", .85, 0, 1), ("intensity", 1, 0, 1), ("storm_severity", 0, 0, 1), ("visibility_m", 5, 2, 100)):
        value = weather.get(key, default)
        if not is_finite_number(value) or not low <= value <= high:
            errors.append(f"{where}: weather {key} must be finite and {low}..{high}")
    for key, default, low, high in (("size", [.025, .075], .005, .15), ("speed", [.45, 1.05], .1, 2)):
        value = weather.get(key, default)
        if (not isinstance(value, list) or len(value) != 2 or not all(is_finite_number(v) for v in value)
                or not low <= value[0] <= value[1] <= high):
            errors.append(f"{where}: weather {key} must be an ordered range in {low}..{high}")
    severity = weather.get("storm_severity", 0)
    storm = is_finite_number(severity) and severity > 0
    if storm:
        shelters = sum(r.get("ceiling", {}).get("kind", "flat") != "open" for r in level.get("rooms", []))
        shelters += sum(w.get("occludes", True) for w in level.get("void_walls", []))
        if shelters > 32:
            errors.append(f"{where}: storm weather supports at most 32 ceiling/roof shelters")
    color = weather.get("fog_color", [.68, .73, .79])
    if not isinstance(color, list) or len(color) != 3 or not all(is_finite_number(v) and 0 <= v <= 1 for v in color):
        errors.append(f"{where}: weather fog_color must contain three finite 0..1 components")
    wind_limit = 20 if storm else .5
    wind = weather.get("wind", [.18, .06])
    if not isinstance(wind, list) or len(wind) != 2 or not all(is_finite_number(v) and abs(v) <= wind_limit for v in wind):
        errors.append(f"{where}: weather wind must contain two finite components within ±{wind_limit}")
    material = weather.get("material", "core:snowflake_01")
    if not isinstance(material, str) or not _ASSET_ID.fullmatch(material.strip()):
        errors.append(f"{where}: weather material must be a logical asset id")


def validate_effects(level: dict, where: str, errors: list[str]) -> None:
    """Effects: count, kind, finite bounds and a bounded particle budget."""
    effects = level.get("effects")
    if effects is None:
        return
    if not isinstance(effects, list):
        errors.append(f"{where}: effects must be an array")
        return
    if len(effects) > MAX_LEVEL_EFFECTS:
        errors.append(f"{where}: too many effects ({len(effects)}; limit {MAX_LEVEL_EFFECTS})")
    for index, effect in enumerate(effects):
        if not isinstance(effect, dict):
            errors.append(f"{where}: effect {index} must be an object")
            continue
        kind = effect.get("kind")
        if kind != "steam":
            errors.append(f"{where}: effect {index} kind must be 'steam', found '{kind}'")
        for key in ("x", "y", "z", "width", "depth", "height", "size", "drift", "lifetime_seconds"):
            value = effect.get(key)
            if value is not None and not is_finite_number(value):
                errors.append(f"{where}: effect {index} {key} must be a finite number")
        count = effect.get("count", 24)
        if not isinstance(count, int) or not 1 <= count <= MAX_EFFECT_PARTICLES:
            errors.append(f"{where}: effect {index} count must be 1..{MAX_EFFECT_PARTICLES}")


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
                if key == "ramps" and field == "rise":
                    # Signed rise encodes the direction, matching the Rust
                    # loader's nonzero-magnitude check and the authoring guide.
                    if abs(float(value)) <= 1e-3:
                        errors.append(f"{entry_where}: rise must have nonzero magnitude (> 0.001 m)")
                elif float(value) <= 0.0:
                    errors.append(f"{entry_where}: {field} must be positive")
            for field in positive:
                if key == "ramps" and field == "rise":
                    continue
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


def _default_volume_ids(level: dict) -> List[str]:
    """The engine's per-instance ids for trigger volumes: authored, else `trigger_<n>`."""
    ids: List[str] = []
    for index, volume in enumerate(level.get("volumes") or []):
        authored = volume.get("id") if isinstance(volume, dict) else None
        if isinstance(authored, str) and authored.strip():
            ids.append(authored.strip())
        else:
            ids.append(f"trigger_{index + 1}")
    return ids


def _entity_facts(kind: str) -> Dict:
    """What one addressable record can do, derived from its kind and components."""
    return {
        "kind": kind,
        "is_prop": False,
        "is_door": False,
        "is_light_fixture": False,
        "is_timer": False,
        "is_volume": False,
        "is_spawn_point": False,
        "is_spawn_template": False,
        "has_interactable": False,
        "interactable_enabled": False,
        "has_animation": False,
        "has_light": False,
        "has_material": False,
        "has_audio": False,
        "has_nav_agent": False,
        "has_ai": False,
        "state_names": set(),
        "material_variants": [],
    }


def _validate_components(context: str, components: object, facts: Dict, errors: List[str]) -> None:
    """Validates one record's typed components and fills the facts actions read.

    Mirrors ``loader::validate_components``: a component the runtime can never
    honour is a named error, never a silent no-op.
    """
    if components is None:
        return
    if not isinstance(components, list):
        errors.append(f"{context}: components must be an array")
        return
    for index, component in enumerate(components):
        entry = f"{context}: component {index}"
        if not isinstance(component, dict):
            errors.append(f"{entry} must be an object")
            continue
        tag = component.get("component")
        if tag not in COMPONENT_TAGS:
            errors.append(f"{entry} has unknown component '{tag}'")
            continue
        if tag == "interactable":
            facts["has_interactable"] = True
            facts["interactable_enabled"] = bool(component.get("enabled", True))
            prompt = component.get("prompt")
            if prompt is not None and (not isinstance(prompt, str) or not prompt.strip()):
                errors.append(f"{entry} prompt must not be blank when specified")
            reach = component.get("reach")
            if reach is not None and (
                not is_finite_number(reach) or reach <= 0.0 or reach > MAX_INTERACTION_REACH_M
            ):
                errors.append(f"{entry} reach must be in (0, {MAX_INTERACTION_REACH_M}]")
            label = component.get("label")
            if label is not None and (not isinstance(label, str) or not label.strip()):
                errors.append(f"{entry} label must not be blank when specified")
        elif tag == "animation":
            facts["has_animation"] = True
            clip = component.get("clip")
            if not isinstance(clip, str) or not clip.strip():
                errors.append(f"{entry} needs a non-blank clip name")
            speed = component.get("speed", 1.0)
            if not is_finite_number(speed) or speed <= 0.0:
                errors.append(f"{entry} speed must be a finite positive number")
        elif tag == "audio":
            facts["has_audio"] = True
            sound = component.get("sound")
            if not isinstance(sound, str) or not sound.strip():
                errors.append(f"{entry} needs a non-blank sound id")
        elif tag == "light":
            facts["has_light"] = True
            if "enabled" in component and not isinstance(component["enabled"], bool):
                errors.append(f"{entry} enabled must be a boolean")
            if "switchable" in component and not isinstance(component["switchable"], bool):
                errors.append(f"{entry} switchable must be a boolean")
            scale = component.get("emission_scale", 1.0)
            if not is_finite_number(scale) or scale < 0.0:
                errors.append(f"{entry} emission_scale must be a finite number >= 0")
        elif tag == "material":
            facts["has_material"] = True
            variants = component.get("variants")
            names: List[str] = []
            if not isinstance(variants, list) or not variants:
                errors.append(f"{entry} needs a non-empty variants array")
            else:
                for variant_index, variant in enumerate(variants):
                    if not isinstance(variant, dict):
                        errors.append(f"{entry} variant {variant_index} must be an object")
                        continue
                    name = variant.get("name")
                    if not isinstance(name, str) or not name.strip():
                        errors.append(f"{entry} variant {variant_index} needs a non-blank name")
                        continue
                    names.append(name.strip())
                    emission = variant.get("emission_scale", 1.0)
                    if not is_finite_number(emission) or emission < 0.0:
                        errors.append(
                            f"{entry} variant {variant_index} emission_scale must be finite >= 0"
                        )
            facts["material_variants"] = names
            current = component.get("current")
            if current is not None and current not in names:
                errors.append(f"{entry} current '{current}' is not a declared variant")
        elif tag == "state":
            name = component.get("name")
            if not isinstance(name, str) or not name.strip():
                errors.append(f"{entry} needs a non-blank state name")
            else:
                facts["state_names"].add(name.strip())
            if "value" not in component:
                errors.append(f"{entry} needs a value")
            elif isinstance(component["value"], (dict, list)):
                errors.append(f"{entry} value must be a boolean, number or string")
        elif tag == "lifetime":
            seconds = component.get("seconds")
            if not is_finite_number(seconds) or seconds <= 0.0:
                errors.append(f"{entry} seconds must be a finite positive number")
        elif tag == "nav_agent":
            facts["has_nav_agent"] = True
            for key in ("radius", "speed_mps", "height", "step_height", "max_slope"):
                value = component.get(key)
                if value is None and key in ("height", "step_height", "max_slope"):
                    # The runtime defaults these three to the reference body.
                    continue
                if not is_finite_number(value) or value <= 0.0:
                    errors.append(f"{entry} {key} must be a finite positive number")
        elif tag == "ai":
            facts["has_ai"] = True
            behavior = component.get("behavior", "idler")
            if behavior not in (
                "idler",
                "wanderer",
                "prey",
                "predator",
                "follower",
            ):
                errors.append(f"{entry} behavior '{behavior}' is not a known behavior")
            role = component.get("role")
            if role is not None and (
                not isinstance(role, str) or not role.strip() or len(role) > 64
            ):
                errors.append(f"{entry} role must be a non-blank tag of at most 64 characters")
            reacts_to = component.get("reacts_to", [])
            if not isinstance(reacts_to, list) or not all(
                isinstance(tag, str) and tag.strip() and len(tag) <= 64 for tag in reacts_to
            ):
                errors.append(f"{entry} reacts_to must be a list of non-blank tags")
            for key, minimum in (
                ("walk_speed", 0.0),
                ("run_speed", 0.0),
                ("sight_range", 0.0),
                ("hearing_range", 0.0),
                ("flee_distance", 0.0),
                ("pursue_distance", 0.0),
                ("wander_radius", 0.0),
                ("idle_seconds", 0.0),
            ):
                value = component.get(key)
                if value is None:
                    continue
                if not is_finite_number(value) or value < minimum:
                    errors.append(f"{entry} {key} must be a finite number >= {minimum}")
                if key in ("walk_speed", "run_speed") and value == 0.0:
                    errors.append(f"{entry} {key} must be positive")
            fov = component.get("sight_fov_degrees")
            if fov is not None and (
                not is_finite_number(fov) or fov < 0.0 or fov > 360.0
            ):
                errors.append(f"{entry} sight_fov_degrees must be between 0 and 360")
            for key in ("catch_radius", "catch_height"):
                value = component.get(key)
                if value is not None and (not is_finite_number(value) or value <= 0.0):
                    errors.append(f"{entry} {key} must be a finite positive number")
            if "can_open_doors" in component and not isinstance(
                component["can_open_doors"], bool
            ):
                errors.append(f"{entry} can_open_doors must be a boolean")
        elif tag == "nav_obstacle":
            size = component.get("size")
            if size is not None and (
                not isinstance(size, list)
                or len(size) != 3
                or not all(is_finite_number(value) and value > 0.0 for value in size)
            ):
                errors.append(f"{entry} size must be [width, height, depth] of positive numbers")
        elif tag == "fade":
            period = component.get("period_seconds", DEFAULT_FADE_PERIOD_SECONDS)
            if (
                not is_finite_number(period)
                or period <= 0.0
                or period > MAX_FADE_PERIOD_SECONDS
            ):
                errors.append(
                    f"{entry} period_seconds must be in (0, {MAX_FADE_PERIOD_SECONDS}]"
                )
            phase = component.get("phase")
            if phase is not None and (
                not is_finite_number(phase) or phase < 0.0 or phase > 1.0
            ):
                errors.append(f"{entry} phase must be between 0 and 1")
            minimum = component.get("min_opacity", 0.0)
            maximum = component.get("max_opacity", 1.0)
            for key, value in (("min_opacity", minimum), ("max_opacity", maximum)):
                if not is_finite_number(value) or value < 0.0 or value > 1.0:
                    errors.append(f"{entry} {key} must be between 0 and 1")
            if (
                is_finite_number(minimum)
                and is_finite_number(maximum)
                and minimum > maximum
            ):
                errors.append(f"{entry} min_opacity must not exceed max_opacity")
            # The proximity contract is all-or-nothing: both radii, in order,
            # and the two fade times only alongside them. Mirrors
            # ``loader::validate_component`` so a level that loads in the
            # engine validates offline too.
            near = component.get("near_radius")
            far = component.get("far_radius")
            if near is None and far is None:
                if "fade_out_seconds" in component or "fade_in_seconds" in component:
                    errors.append(
                        f"{entry} authors fade_out_seconds/fade_in_seconds without "
                        "near_radius and far_radius"
                    )
            elif near is None:
                errors.append(f"{entry} authors far_radius without near_radius")
            elif far is None:
                errors.append(f"{entry} authors near_radius without far_radius")
            else:
                if not is_finite_number(near) or near <= 0.0:
                    errors.append(
                        f"{entry} near_radius must be a finite number greater than 0"
                    )
                elif not is_finite_number(far) or far <= near:
                    errors.append(
                        f"{entry} far_radius must be a finite number greater than near_radius"
                    )
                for key in ("fade_out_seconds", "fade_in_seconds"):
                    value = component.get(key)
                    if value is not None and (
                        not is_finite_number(value)
                        or value <= 0.0
                        or value > MAX_FADE_SECONDS
                    ):
                        errors.append(
                            f"{entry} {key} must be in (0, {MAX_FADE_SECONDS}]"
                        )
        elif tag == "glow":
            color = component.get("color", [1.0, 0.86, 0.6])
            if (
                not isinstance(color, list)
                or len(color) != 3
                or not all(
                    is_finite_number(channel) and 0.0 <= channel <= 1.0
                    for channel in color
                )
            ):
                errors.append(f"{entry} color must be three channels between 0 and 1")
            intensity = component.get("intensity", 0.5)
            if (
                not is_finite_number(intensity)
                or intensity < 0.0
                or intensity > MAX_GLOW_INTENSITY
            ):
                errors.append(f"{entry} intensity must be in [0, {MAX_GLOW_INTENSITY}]")
            span = component.get("range", 3.0)
            if (
                not is_finite_number(span)
                or span < MIN_GLOW_RANGE_M
                or span > MAX_GLOW_RANGE_M
            ):
                errors.append(
                    f"{entry} range must be in [{MIN_GLOW_RANGE_M}, {MAX_GLOW_RANGE_M}]"
                )
            socket = component.get("socket")
            if socket is not None and (not isinstance(socket, str) or not socket.strip()):
                errors.append(f"{entry} socket must not be blank when specified")
            offset = component.get("offset")
            if offset is not None and (
                not isinstance(offset, list)
                or len(offset) != 3
                or not all(
                    is_finite_number(axis) and abs(axis) <= MAX_GLOW_OFFSET_M
                    for axis in offset
                )
            ):
                errors.append(
                    f"{entry} offset must be three finite values within +/-{MAX_GLOW_OFFSET_M}"
                )
        elif tag in ("steam", "water") and "enabled" in component:
            if not isinstance(component["enabled"], bool):
                errors.append(f"{entry} enabled must be a boolean")


def _validate_ai_body(context: str, facts: Dict, errors: List[str]) -> None:
    """Mirrors the loader: an `ai` entity must author a navigation body."""
    if facts.get("has_ai") and not facts.get("has_nav_agent"):
        errors.append(
            f"{context} authors an `ai` component without a `nav_agent` body"
        )


def _validate_instance_id(
    seen: Dict[str, str], ident: object, context: str, namespace: str, errors: List[str]
) -> Optional[str]:
    """Well-formedness and uniqueness for one authored/defaulted id."""
    if not isinstance(ident, str) or not ident.strip():
        errors.append(f"{context} id must be a non-empty, well-formed identifier")
        return None
    trimmed = ident.strip()
    if not _ASSET_ID.match(trimmed):
        errors.append(f"{context} id '{trimmed}' must be a well-formed identifier")
        return None
    if trimmed in seen:
        errors.append(f"{context} id '{trimmed}' duplicates {seen[trimmed]}; {namespace} ids must be unique")
        return None
    seen[trimmed] = context
    return trimmed


def _index_level_entities(level: dict, where: str, errors: List[str]) -> Dict:
    """Every addressable id and capability set the action/condition checks read.

    One instance namespace covers props, doors, ceiling fixtures, trigger
    volumes, timers and spawn points; sequences, spawn templates and spawn
    groups are separate resource namespaces, exactly as ``loader::LevelIndex``
    resolves them.
    """
    entities: Dict[str, Dict] = {}
    templates: Dict[str, Dict] = {}
    sequences: Dict[str, Dict] = {}
    groups: Dict[str, Dict] = {}
    points: Dict[str, Dict] = {}
    prop_ids = _resolve_prop_ids(level)
    seen: Dict[str, str] = {}

    for index, prop in enumerate(level.get("props") or []):
        if not isinstance(prop, dict) or index >= len(prop_ids):
            continue
        context = f"prop {index} ('{prop_ids[index]}')"
        ident = _validate_instance_id(seen, prop_ids[index], context, "instance", errors)
        if ident is None:
            continue
        facts = _entity_facts("prop")
        facts["is_prop"] = True
        if prop.get("display_name") is not None and (
            not isinstance(prop["display_name"], str) or not prop["display_name"].strip()
        ):
            errors.append(f"{context} display_name must not be blank when specified")
        _validate_components(context, prop.get("components"), facts, errors)
        _validate_ai_body(context, facts, errors)
        entities[ident] = facts

    for index, fixture in enumerate(level.get("ceiling_lights") or []):
        if not isinstance(fixture, dict):
            continue
        authored = fixture.get("id")
        if not isinstance(authored, str) or not authored.strip():
            continue
        context = f"ceiling light {index} ('{authored.strip()}')"
        ident = _validate_instance_id(seen, authored, context, "instance", errors)
        if ident is None:
            continue
        facts = _entity_facts("ceiling light")
        facts["is_light_fixture"] = True
        entities[ident] = facts

    for index, door in enumerate(level.get("doors") or []):
        if not isinstance(door, dict):
            continue
        authored = door.get("id")
        context = f"door {index} ('{authored}')"
        ident = _validate_instance_id(seen, authored, context, "instance", errors)
        if ident is None:
            continue
        facts = _entity_facts("door")
        facts["is_door"] = True
        _validate_components(context, door.get("components"), facts, errors)
        entities[ident] = facts

    for index, ident in enumerate(_default_volume_ids(level)):
        context = f"trigger volume {index} ('{ident}')"
        resolved = _validate_instance_id(seen, ident, context, "instance", errors)
        if resolved is None:
            continue
        facts = _entity_facts("trigger volume")
        facts["is_volume"] = True
        entities[resolved] = facts

    # Water volumes and effect emitters are entities too: they carry no
    # components, but an `enable`/`disable` action must be able to name one,
    # exactly as the runtime's controllers resolve them.
    for index, volume in enumerate(level.get("water") or []):
        if not isinstance(volume, dict):
            continue
        ident = f"water_{index + 1}"
        context = f"water volume {index} ('{ident}')"
        resolved = _validate_instance_id(seen, ident, context, "instance", errors)
        if resolved is not None:
            entities[resolved] = _entity_facts("water volume")
    for index, effect in enumerate(level.get("effects") or []):
        if not isinstance(effect, dict):
            continue
        authored = effect.get("id")
        ident = (
            str(authored).strip()
            if isinstance(authored, str) and authored.strip()
            else f"effect_{index + 1}"
        )
        context = f"effect {index} ('{ident}')"
        resolved = _validate_instance_id(seen, ident, context, "instance", errors)
        if resolved is not None:
            entities[resolved] = _entity_facts("effect")

    for index, timer in enumerate(level.get("timers") or []):
        if not isinstance(timer, dict):
            continue
        context = f"timer {index} ('{timer.get('id')}')"
        ident = _validate_instance_id(seen, timer.get("id"), context, "instance", errors)
        if ident is None:
            continue
        facts = _entity_facts("timer")
        facts["is_timer"] = True
        entities[ident] = facts

    for index, point in enumerate(level.get("spawn_points") or []):
        if not isinstance(point, dict):
            continue
        context = f"spawn point {index} ('{point.get('id')}')"
        ident = _validate_instance_id(seen, point.get("id"), context, "instance", errors)
        if ident is None:
            continue
        facts = _entity_facts("spawn point")
        facts["is_spawn_point"] = True
        points[ident] = facts
        entities[ident] = facts

    sequence_seen: Dict[str, str] = {}
    for index, sequence in enumerate(level.get("sequences") or []):
        if not isinstance(sequence, dict):
            continue
        context = f"sequence {index} ('{sequence.get('id')}')"
        ident = _validate_instance_id(
            sequence_seen, sequence.get("id"), context, "sequence", errors
        )
        if ident is not None:
            sequences[ident] = {"id": ident, "def": sequence}

    template_seen: Dict[str, str] = {}
    for index, template in enumerate(level.get("spawn_templates") or []):
        if not isinstance(template, dict):
            continue
        context = f"spawn template {index} ('{template.get('id')}')"
        ident = _validate_instance_id(
            template_seen, template.get("id"), context, "spawn template", errors
        )
        if ident is None:
            continue
        facts = _entity_facts("spawn template")
        facts["is_spawn_template"] = True
        _validate_components(context, template.get("components"), facts, errors)
        _validate_ai_body(context, facts, errors)
        templates[ident] = facts

    group_seen: Dict[str, str] = {}
    for index, group in enumerate(level.get("spawn_groups") or []):
        if not isinstance(group, dict):
            continue
        context = f"spawn group {index} ('{group.get('id')}')"
        ident = _validate_instance_id(group_seen, group.get("id"), context, "spawn group", errors)
        if ident is not None:
            groups[ident] = group

    return {
        "entities": entities,
        "templates": templates,
        "sequences": sequences,
        "groups": groups,
        "points": points,
        "prop_ids": prop_ids,
    }


def _resolve_subject(
    target: object,
    implicit: Optional[tuple],
    index: Dict,
    context: str,
    tag: str,
    errors: List[str],
) -> Optional[tuple]:
    """The subject one action acts on, or None when there is nothing to check."""
    if target is not None:
        if not isinstance(target, str) or not target.strip():
            errors.append(f"{context} ('{tag}') target must not be blank when specified")
            return None
        resolved = target.strip()
        facts = index["entities"].get(resolved) or index["templates"].get(resolved)
        if facts is None:
            errors.append(f"{context} ('{tag}') targets unknown entity '{resolved}'")
            return None
        return resolved, facts
    return implicit


def _validate_conditions(context: str, when: object, index: Dict, errors: List[str]) -> None:
    """Every condition in one binding: a known check and a resolvable target."""
    if when is None:
        return
    if not isinstance(when, list):
        errors.append(f"{context}: when must be an array of conditions")
        return
    for position, condition in enumerate(when):
        entry = f"{context} condition {position}"
        if not isinstance(condition, dict):
            errors.append(f"{entry} must be an object")
            continue
        check = condition.get("check")
        if check not in CONDITION_CHECKS:
            errors.append(f"{entry} has unknown check '{check}'")
            continue
        target = condition.get("target")
        if not isinstance(target, str) or not target.strip():
            errors.append(f"{entry} ('{check}') needs a target")
        elif target.strip() not in index["entities"]:
            errors.append(f"{entry} ('{check}') targets unknown entity '{target.strip()}'")
        if check == "state":
            name = condition.get("name")
            if not isinstance(name, str) or not name.strip():
                errors.append(f"{entry} ('state') needs a state name")
            if "equals" not in condition:
                errors.append(f"{entry} ('state') needs an equals value")


def _validate_actions(
    context: str,
    actions: object,
    implicit: Optional[tuple],
    index: Dict,
    errors: List[str],
) -> None:
    """One source's actions: bounded, known tags, resolvable capability-fit targets."""
    if not isinstance(actions, list) or not actions:
        errors.append(f"{context} must declare 1..{MAX_ACTIONS_PER_SOURCE} actions")
        return
    if len(actions) > MAX_ACTIONS_PER_SOURCE:
        errors.append(f"{context} declares {len(actions)} actions; the limit is {MAX_ACTIONS_PER_SOURCE}")
    for position, action in enumerate(actions):
        if not isinstance(action, dict):
            errors.append(f"{context} action {position} must be an object")
            continue
        tag = action.get("action")
        entry = f"{context} action {position}"
        if tag not in ACTION_TAGS:
            errors.append(f"{entry} has unknown action '{tag}'")
            continue
        if tag not in IMPLEMENTED_ACTIONS:
            errors.append(f"{entry} ('{tag}') is not implemented yet")
            continue

        def subject():
            return _resolve_subject(action.get("target"), implicit, index, entry, tag, errors)

        def require(requirement: str, predicate) -> None:
            resolved = subject()
            if resolved is None:
                return
            subject_id, facts = resolved
            if not predicate(facts):
                errors.append(
                    f"{entry} ('{tag}') targets '{subject_id}', a {facts['kind']}; "
                    f"`{tag}` requires {requirement}"
                )

        if tag in ("open", "close", "lock", "unlock"):
            require("a door target", lambda facts: facts["is_door"])
        elif tag == "toggle":
            require(
                "a door or a light-capable target",
                lambda facts: facts["is_door"] or facts["is_light_fixture"] or facts["has_light"],
            )
        elif tag in ("enable", "disable"):
            subject()
        elif tag == "set_light":
            if not isinstance(action.get("on"), bool):
                errors.append(f"{entry} ('set_light') needs a boolean `on`")
            require(
                "a light-capable target",
                lambda facts: facts["is_light_fixture"] or facts["has_light"],
            )
        elif tag in ("play_animation", "toggle_animation"):
            require("a target with an `animation` component", lambda facts: facts["has_animation"])
            clip = action.get("clip")
            if clip is not None and (not isinstance(clip, str) or not clip.strip()):
                errors.append(f"{entry} ('{tag}') clip must not be blank when specified")
        elif tag == "change_material":
            require("a target with a `material` component", lambda facts: facts["has_material"])
            variant = action.get("variant")
            if not isinstance(variant, str) or not variant.strip():
                errors.append(f"{entry} ('change_material') needs a non-empty variant name")
            else:
                resolved = subject()
                if resolved is not None:
                    _, facts = resolved
                    if facts["has_material"] and variant.strip() not in facts["material_variants"]:
                        errors.append(
                            f"{entry} ('change_material') targets a `material` component "
                            f"that declares no variant '{variant.strip()}'"
                        )
        elif tag == "move_object":
            for key in ("x", "z"):
                if not is_finite_number(action.get(key)):
                    errors.append(f"{entry} ('move_object') {key} must be a finite number")
            if action.get("y") is not None and not is_finite_number(action.get("y")):
                errors.append(f"{entry} ('move_object') y must be a finite number when specified")
            speed = action.get("speed")
            if speed is not None and (not is_finite_number(speed) or speed <= 0.0):
                errors.append(f"{entry} ('move_object') speed must be a finite positive number")
            require(
                "a door or a spawn-template instance",
                lambda facts: facts["is_door"] or facts["is_spawn_template"],
            )
        elif tag == "set_state":
            name = action.get("name")
            if not isinstance(name, str) or not name.strip():
                errors.append(f"{entry} ('set_state') needs a non-empty state name")
            elif "value" not in action:
                errors.append(f"{entry} ('set_state') needs a value")
            elif isinstance(action["value"], (dict, list)):
                errors.append(f"{entry} ('set_state') value must be a boolean, number or string")
            if isinstance(name, str) and name.strip():
                trimmed = name.strip()

                def state_capable(facts, trimmed=trimmed):
                    return (
                        trimmed in facts["state_names"]
                        or facts["is_timer"]
                        or facts["is_volume"]
                    )

                require(
                    f"an authored state named '{trimmed}' (or a timer/volume target)",
                    state_capable,
                )
        elif tag == "toggle_label":
            require("a placed prop target", lambda facts: facts["is_prop"])
        elif tag == "start_sequence":
            sequence = action.get("sequence")
            if not isinstance(sequence, str) or not sequence.strip():
                errors.append(f"{entry} ('start_sequence') needs a non-empty sequence id")
            elif sequence.strip() not in index["sequences"]:
                errors.append(f"{entry} ('start_sequence') references unknown sequence '{sequence.strip()}'")
            subject()
        elif tag == "stop_sequence":
            subject()
        elif tag in ("start_timer", "stop_timer"):
            require("a timer target", lambda facts: facts["is_timer"])
            if tag == "start_timer":
                seconds = action.get("seconds")
                if seconds is not None and (not is_finite_number(seconds) or seconds <= 0.0):
                    errors.append(
                        f"{entry} ('start_timer') seconds override must be a finite positive number"
                    )
        elif tag == "spawn_entity":
            template = action.get("template")
            if template is not None and (not isinstance(template, str) or not template.strip()):
                errors.append(f"{entry} ('spawn_entity') template must not be blank when specified")
            elif isinstance(template, str) and template.strip() not in index["templates"]:
                errors.append(
                    f"{entry} ('spawn_entity') references unknown spawn template '{template.strip()}'"
                )
            group = action.get("group")
            if group is not None and (not isinstance(group, str) or not group.strip()):
                errors.append(f"{entry} ('spawn_entity') group must not be blank when specified")
            elif isinstance(group, str) and group.strip() not in index["groups"]:
                errors.append(f"{entry} ('spawn_entity') references unknown spawn group '{group.strip()}'")
            name = action.get("name")
            if name is not None and (not isinstance(name, str) or not name.strip()):
                errors.append(f"{entry} ('spawn_entity') name must not be blank when specified")
            point = action.get("point")
            if point is not None and (not isinstance(point, str) or not point.strip()):
                errors.append(f"{entry} ('spawn_entity') point must not be blank when specified")
            elif isinstance(point, str):
                if point.strip() not in index["points"]:
                    errors.append(f"{entry} ('spawn_entity') targets unknown spawn point '{point.strip()}'")
            elif isinstance(template, str) and template.strip():
                errors.append(
                    f"{entry} ('spawn_entity') names a template without a point; a template "
                    "alone has no world position, so name the spawn point to spawn at"
                )
            else:
                resolved = subject()
                if resolved is not None and not resolved[1]["is_spawn_point"]:
                    errors.append(
                        f"{entry} ('spawn_entity') targets '{resolved[0]}', a "
                        f"{resolved[1]['kind']}; a spawn without a point may only come from a spawn point"
                    )
        elif tag == "despawn_entity":
            target = action.get("target")
            if not isinstance(target, str) or not target.strip():
                errors.append(f"{entry} ('despawn_entity') needs a non-empty target")
            elif (
                target.strip() not in index["entities"]
                and target.strip() not in index["templates"]
                and target.strip() not in index["groups"]
            ):
                errors.append(
                    f"{entry} ('despawn_entity') targets '{target.strip()}'; needs an authored "
                    "entity id or spawn group id"
                )


def _validate_bindings(
    context: str,
    bindings: object,
    implicit: Optional[tuple],
    index: Dict,
    errors: List[str],
) -> None:
    """One record's event bindings: bounded, known event kinds, executable actions."""
    if bindings is None:
        return
    if not isinstance(bindings, list):
        errors.append(f"{context}: bindings must be an array")
        return
    if len(bindings) > MAX_BINDINGS_PER_ENTITY:
        errors.append(f"{context} declares {len(bindings)} bindings; the limit is {MAX_BINDINGS_PER_ENTITY}")
    for position, binding in enumerate(bindings):
        entry = f"{context} binding {position}"
        if not isinstance(binding, dict):
            errors.append(f"{entry} must be an object")
            continue
        on = binding.get("on")
        if on not in EVENT_KINDS:
            errors.append(f"{entry} has unknown event kind '{on}'")
        elif implicit is not None:
            facts = implicit[1]
            if on in ("enter_volume", "exit_volume") and not facts["is_volume"]:
                errors.append(f"{entry} listens for '{on}', but {context} is not a trigger volume")
            # A `timer` cue may come from an authored timer or from a
            # sequence's `emit` step: any record may listen for it.
            elif on == "animation_complete" and not facts["has_animation"]:
                errors.append(f"{entry} listens for 'animation_complete', but {context} has no animation component")
            elif on == "interact" and not (facts["has_interactable"] and facts["interactable_enabled"]):
                errors.append(
                    f"{entry} listens for 'interact', but {context} has no enabled interactable component"
                )
        binding_id = binding.get("id")
        if binding_id is not None and (not isinstance(binding_id, str) or not binding_id.strip()):
            errors.append(f"{entry} id must not be blank when specified")
        key = binding.get("key")
        if key is not None and (not isinstance(key, str) or not key.strip()):
            errors.append(f"{entry} key must not be blank when specified")
        once = binding.get("once", False)
        if not isinstance(once, bool):
            errors.append(f"{entry} once must be a boolean")
        cooldown = binding.get("cooldown_seconds", 0.0)
        if not is_finite_number(cooldown) or cooldown < 0.0:
            errors.append(f"{entry} cooldown_seconds must be a finite number >= 0")
        _validate_conditions(entry, binding.get("when"), index, errors)
        _validate_actions(entry, binding.get("actions"), implicit, index, errors)


def _validate_volumes(level: dict, where: str, errors: List[str]) -> None:
    """Trigger volumes: count, footprint, vertical band, room overlap.

    The volume's actions live in its bindings and are validated with every
    other binding; this function mirrors ``loader::validate_volumes``.
    """
    volumes = level.get("volumes")
    if volumes is None:
        return
    if not isinstance(volumes, list):
        errors.append(f"{where}: volumes must be an array")
        return
    if len(volumes) > MAX_AREA_TRIGGERS:
        errors.append(f"{where}: too many trigger volumes ({len(volumes)}; limit {MAX_AREA_TRIGGERS})")
    for index, volume in enumerate(volumes):
        if not isinstance(volume, dict):
            errors.append(f"{where}: trigger volume {index} must be an object")
            continue
        width = volume.get("width")
        depth = volume.get("depth")
        if not is_finite_number(width) or not is_finite_number(depth) or width <= 0.0 or depth <= 0.0:
            errors.append(f"{where}: trigger volume {index} width and depth must be positive numbers")
        bottom = volume.get("bottom_y")
        top = volume.get("top_y")
        if bottom is not None and not is_finite_number(bottom):
            errors.append(f"{where}: trigger volume {index} bottom_y must be a finite number")
        if top is not None and not is_finite_number(top):
            errors.append(f"{where}: trigger volume {index} top_y must be a finite number")
        if is_finite_number(bottom) and is_finite_number(top) and top <= bottom:
            errors.append(f"{where}: trigger volume {index} top_y must be above its bottom_y")
        if is_finite_number(width) and is_finite_number(depth) and width > 0.0 and depth > 0.0:
            footprint = {
                "x": volume.get("x", 0.0),
                "z": volume.get("z", 0.0),
                "width": width,
                "depth": depth,
            }
            if not wall_touches_any_room(level, footprint):
                errors.append(f"{where}: trigger volume {index} lies outside every room section")


def _validate_timers(level: dict, where: str, index: Dict, errors: List[str]) -> None:
    """Authored timers: a positive period and structurally valid bindings."""
    timers = level.get("timers")
    if timers is None:
        return
    if not isinstance(timers, list):
        errors.append(f"{where}: timers must be an array")
        return
    for position, timer in enumerate(timers):
        context = f"{where}: timer {position}"
        if not isinstance(timer, dict):
            errors.append(f"{context} must be an object")
            continue
        ident = timer.get("id")
        if not isinstance(ident, str) or not ident.strip():
            errors.append(f"{context} needs an id")
            continue
        ident = ident.strip()
        seconds = timer.get("seconds")
        if not is_finite_number(seconds) or seconds <= 0.0:
            errors.append(f"{context} ('{ident}') seconds must be a finite positive number")
        for flag in ("repeat", "autostart"):
            if flag in timer and not isinstance(timer[flag], bool):
                errors.append(f"{context} ('{ident}') {flag} must be a boolean")
        facts = index["entities"].get(ident)
        _validate_bindings(
            f"{where}: timer ('{ident}')",
            timer.get("bindings"),
            (ident, facts) if facts else None,
            index,
            errors,
        )


def _validate_spawns(level: dict, where: str, index: Dict, errors: List[str]) -> None:
    """Spawn templates, points and groups, including the at-most-one-active rule."""
    templates = level.get("spawn_templates") or []
    points = level.get("spawn_points") or []
    groups = level.get("spawn_groups") or []
    if len(templates) > MAX_LEVEL_SPAWN_TEMPLATES:
        errors.append(f"{where}: too many spawn templates ({len(templates)}; limit {MAX_LEVEL_SPAWN_TEMPLATES})")
    if len(points) > MAX_LEVEL_SPAWN_POINTS:
        errors.append(f"{where}: too many spawn points ({len(points)}; limit {MAX_LEVEL_SPAWN_POINTS})")
    if len(groups) > MAX_LEVEL_SPAWN_GROUPS:
        errors.append(f"{where}: too many spawn groups ({len(groups)}; limit {MAX_LEVEL_SPAWN_GROUPS})")

    for position, template in enumerate(templates):
        context = f"{where}: spawn template {position}"
        if not isinstance(template, dict):
            errors.append(f"{context} must be an object")
            continue
        ident = template.get("id")
        if not isinstance(ident, str) or not ident.strip():
            errors.append(f"{context} needs an id")
            continue
        ident = ident.strip()
        model = template.get("model")
        if not isinstance(model, str) or not model.strip():
            errors.append(f"{context} ('{ident}') needs a model")
        scale = template.get("scale", 1.0)
        if not is_finite_number(scale) or scale <= 0.0:
            errors.append(f"{context} ('{ident}') scale must be a finite positive number")
        lifetime = template.get("lifetime_seconds")
        if lifetime is not None and (not is_finite_number(lifetime) or lifetime <= 0.0):
            errors.append(f"{context} ('{ident}') lifetime_seconds must be a finite positive number")
        facts = index["templates"].get(ident)
        _validate_bindings(f"{where}: spawn template ('{ident}')", template.get("bindings"), (ident, facts), index, errors)

    for position, point in enumerate(points):
        context = f"{where}: spawn point {position}"
        if not isinstance(point, dict):
            errors.append(f"{context} must be an object")
            continue
        ident = point.get("id")
        if not isinstance(ident, str) or not ident.strip():
            errors.append(f"{context} needs an id")
            continue
        ident = ident.strip()
        for key in ("x", "z"):
            if not is_finite_number(point.get(key, 0.0)):
                errors.append(f"{context} ('{ident}') {key} must be a finite number")
        if point.get("y") is not None and not is_finite_number(point.get("y")):
            errors.append(f"{context} ('{ident}') y must be a finite number when specified")
        if point.get("yaw_degrees") is not None and not is_finite_number(point.get("yaw_degrees")):
            errors.append(f"{context} ('{ident}') yaw_degrees must be a finite number")
        template = point.get("template")
        if not isinstance(template, str) or not template.strip():
            errors.append(f"{context} ('{ident}') needs a template")
        elif template.strip() not in index["templates"]:
            errors.append(f"{context} ('{ident}') references unknown spawn template '{template.strip()}'")
        group = point.get("group")
        if group is not None:
            if not isinstance(group, str) or not group.strip():
                errors.append(f"{context} ('{ident}') group must not be blank when specified")
            elif group.strip() not in index["groups"]:
                errors.append(f"{context} ('{ident}') references unknown spawn group '{group.strip()}'")
        facts = index["entities"].get(ident)
        _validate_bindings(
            f"{where}: spawn point ('{ident}')",
            point.get("bindings"),
            (ident, facts) if facts else None,
            index,
            errors,
        )

    for position, group in enumerate(groups):
        context = f"{where}: spawn group {position}"
        if not isinstance(group, dict):
            errors.append(f"{context} must be an object")
            continue
        if not isinstance(group.get("id"), str) or not group["id"].strip():
            errors.append(f"{context} needs an id")
        if "at_most_one_active" in group and not isinstance(group["at_most_one_active"], bool):
            errors.append(f"{context} at_most_one_active must be a boolean")


def _validate_sequences(level: dict, where: str, index: Dict, errors: List[str]) -> None:
    """Authored sequences: bounded steps, known step tags and their required fields."""
    sequences = level.get("sequences")
    if sequences is None:
        return
    if not isinstance(sequences, list):
        errors.append(f"{where}: sequences must be an array")
        return
    if len(sequences) > MAX_LEVEL_SEQUENCES:
        errors.append(f"{where}: too many sequences ({len(sequences)}; limit {MAX_LEVEL_SEQUENCES})")
    for position, sequence in enumerate(sequences):
        context = f"{where}: sequence {position}"
        if not isinstance(sequence, dict):
            errors.append(f"{context} must be an object")
            continue
        ident = sequence.get("id")
        if not isinstance(ident, str) or not ident.strip():
            errors.append(f"{context} needs an id")
            continue
        ident = ident.strip()
        looped = sequence.get("looped", False)
        if not isinstance(looped, bool):
            errors.append(f"{context} ('{ident}') looped must be a boolean")
        steps = sequence.get("steps")
        if not isinstance(steps, list) or not steps:
            errors.append(f"{context} ('{ident}') must declare at least one step")
            continue
        if len(steps) > MAX_SEQUENCE_STEPS:
            errors.append(f"{context} ('{ident}') declares {len(steps)} steps; the limit is {MAX_SEQUENCE_STEPS}")
        for step_position, step in enumerate(steps):
            entry = f"{context} ('{ident}') step {step_position}"
            if not isinstance(step, dict):
                errors.append(f"{entry} must be an object")
                continue
            tag = step.get("step")
            if tag not in SEQUENCE_STEP_TAGS:
                errors.append(f"{entry} has unknown step '{tag}'")
                continue
            if tag == "action":
                action = step.get("action")
                if not isinstance(action, dict):
                    errors.append(f"{entry} ('action') needs an action object")
                else:
                    _validate_actions(entry, [action], None, index, errors)
            elif tag == "wait":
                seconds = step.get("seconds")
                if not is_finite_number(seconds) or not 0.0 <= seconds <= MAX_SEQUENCE_WAIT_S:
                    errors.append(f"{entry} ('wait') seconds must be between 0 and {MAX_SEQUENCE_WAIT_S}")
            elif tag == "move":
                for key in ("x", "z", "speed"):
                    if not is_finite_number(step.get(key)):
                        errors.append(f"{entry} ('move') {key} must be a finite number")
                speed = step.get("speed")
                if is_finite_number(speed) and speed <= 0.0:
                    errors.append(f"{entry} ('move') speed must be positive")
                if step.get("y") is not None and not is_finite_number(step.get("y")):
                    errors.append(f"{entry} ('move') y must be a finite number when specified")
            elif tag == "face":
                if not is_finite_number(step.get("yaw_degrees")):
                    errors.append(f"{entry} ('face') yaw_degrees must be a finite number")
            elif tag == "wait_animation":
                timeout = step.get("timeout", 0.0)
                if not is_finite_number(timeout) or not 0.0 <= timeout <= MAX_SEQUENCE_ANIMATION_TIMEOUT_S:
                    errors.append(
                        f"{entry} ('wait_animation') timeout must be between 0 and "
                        f"{MAX_SEQUENCE_ANIMATION_TIMEOUT_S}"
                    )
                clip = step.get("clip")
                if clip is not None and (not isinstance(clip, str) or not clip.strip()):
                    errors.append(f"{entry} ('wait_animation') clip must not be blank when specified")
            elif tag == "emit":
                if step.get("on") not in EVENT_KINDS:
                    errors.append(f"{entry} ('emit') on must be a known event kind")
                key = step.get("key")
                if key is not None and (not isinstance(key, str) or not key.strip()):
                    errors.append(f"{entry} ('emit') key must not be blank when specified")
            elif tag == "set_state":
                name = step.get("name")
                if not isinstance(name, str) or not name.strip():
                    errors.append(f"{entry} ('set_state') needs a state name")
                if "value" not in step:
                    errors.append(f"{entry} ('set_state') needs a value")
                elif isinstance(step["value"], (dict, list)):
                    errors.append(f"{entry} ('set_state') value must be a boolean, number or string")


def validate_interactions(level: dict, where: str, errors: List[str]) -> None:
    """Instance ids, typed components, event bindings, timers, sequences and volumes.

    Mirrors the v3 loader contract (``loader::validate_instance_ids``,
    ``validate_bindings``, ``validate_volumes``, ``validate_timers``,
    ``validate_sequences``, ``validate_spawns``): well-formed unique ids,
    known component and event kinds, 1..8 typed actions per binding with
    resolvable capability-fit targets, real trigger volumes, well-formed
    timers, sequences and spawn definitions. ``play_sound``/``stop_sound`` are
    parsed but unimplemented and are rejected by name; ``play_audio`` and
    unknown tags are rejected.
    """
    index = _index_level_entities(level, where, errors)

    for position, prop in enumerate(level.get("props") or []):
        if not isinstance(prop, dict):
            continue
        prop_id = index["prop_ids"][position] if position < len(index["prop_ids"]) else None
        facts = index["entities"].get(prop_id) if prop_id else None
        _validate_bindings(
            f"prop {position}" + (f" ('{prop_id}')" if prop_id else ""),
            prop.get("bindings"),
            (prop_id, facts) if prop_id and facts else None,
            index,
            errors,
        )
    for position, door in enumerate(level.get("doors") or []):
        if not isinstance(door, dict):
            continue
        door_id = str(door.get("id", "")).strip() or None
        facts = index["entities"].get(door_id) if door_id else None
        _validate_bindings(
            f"door {position}" + (f" ('{door_id}')" if door_id else ""),
            door.get("bindings"),
            (door_id, facts) if door_id and facts else None,
            index,
            errors,
        )
    for position, fixture in enumerate(level.get("ceiling_lights") or []):
        if not isinstance(fixture, dict):
            continue
        fixture_id = fixture.get("id")
        if not isinstance(fixture_id, str) or not fixture_id.strip():
            continue
        fixture_id = fixture_id.strip()
        facts = index["entities"].get(fixture_id)
        _validate_bindings(
            f"ceiling light {position} ('{fixture_id}')",
            fixture.get("bindings"),
            (fixture_id, facts) if facts else None,
            index,
            errors,
        )
    volume_ids = _default_volume_ids(level)
    for position, volume in enumerate(level.get("volumes") or []):
        if not isinstance(volume, dict):
            continue
        volume_id = volume_ids[position] if position < len(volume_ids) else f"trigger_{position + 1}"
        facts = index["entities"].get(volume_id)
        _validate_bindings(
            f"trigger volume {position} ('{volume_id}')",
            volume.get("bindings"),
            (volume_id, facts) if facts else None,
            index,
            errors,
        )
    for position, effect in enumerate(level.get("effects") or []):
        if not isinstance(effect, dict):
            continue
        effect_id = effect.get("id")
        label = f"effect {position}" + (
            f" ('{effect_id}')" if isinstance(effect_id, str) and effect_id.strip() else ""
        )
        _validate_bindings(
            label,
            effect.get("bindings"),
            ("the effect", _entity_facts("effect")),
            index,
            errors,
        )

    _validate_timers(level, where, index, errors)
    _validate_sequences(level, where, index, errors)
    _validate_spawns(level, where, index, errors)
    _validate_volumes(level, where, errors)

    validate_entity_routes(level, where, errors, index["prop_ids"])


def water_footprints(level: dict) -> List[dict]:
    """Every authored water volume as a resolved footprint for containment.

    Mirrors ``WaterVolumeDef::bounds``: a rectangle spans `x..x + width` and
    `z..z + depth`; a circle is the disc of `radius` inscribed in
    `x..x + 2 * radius`. Malformed volumes are skipped; the schema check names
    them separately.
    """
    footprints: List[dict] = []
    for volume in level.get("water") or []:
        if not isinstance(volume, dict):
            continue
        x, z = volume.get("x"), volume.get("z")
        if not (is_finite_number(x) and is_finite_number(z)):
            continue
        shape = volume.get("shape", "rect")
        if shape == "circle":
            radius = volume.get("radius")
            if not is_finite_number(radius) or radius <= 0.0:
                continue
            footprints.append(
                {
                    "shape": "circle",
                    "x0": min(x, x + 2.0 * radius),
                    "x1": max(x, x + 2.0 * radius),
                    "z0": min(z, z + 2.0 * radius),
                    "z1": max(z, z + 2.0 * radius),
                    "radius": radius,
                }
            )
        else:
            width, depth = volume.get("width"), volume.get("depth")
            if (
                not is_finite_number(width)
                or not is_finite_number(depth)
                or width <= 0.0
                or depth <= 0.0
            ):
                continue
            footprints.append(
                {
                    "shape": "rect",
                    "x0": min(x, x + width),
                    "x1": max(x, x + width),
                    "z0": min(z, z + depth),
                    "z1": max(z, z + depth),
                    "radius": 0.0,
                }
            )
    return footprints


def footprint_contains_disc(footprint: dict, x: float, z: float, radius: float) -> bool:
    """True when a disc fits inside one resolved footprint.

    Mirrors ``WaterVolume::contains_disc``: an axis-aligned rectangle needs
    the disc inside the box, a circle needs it inside the disc. The circular
    test carries a micrometre of slack so an exactly-rim-riding authored
    float is not rejected by a last-bit difference between this checker and
    the engine's squared-distance comparison.
    """
    if footprint["shape"] == "rect":
        return (
            x - radius >= footprint["x0"]
            and x + radius <= footprint["x1"]
            and z - radius >= footprint["z0"]
            and z + radius <= footprint["z1"]
        )
    centre_x = (footprint["x0"] + footprint["x1"]) / 2.0
    centre_z = (footprint["z0"] + footprint["z1"]) / 2.0
    clearance = footprint["radius"] - radius
    return clearance >= -1.0e-6 and math.hypot(
        x - centre_x, z - centre_z
    ) <= clearance + 1.0e-6


def validate_water_shapes(level: dict, where: str, errors: List[str]) -> None:
    """Water footprint shapes: the rectangle and circle authoring contracts.

    Mirrors ``loader::validate_water``: a rectangle needs ``width``/``depth``
    and no ``radius``; a circle needs a finite positive ``radius`` and may
    author ``width``/``depth`` only as its own diameter, because the bounding
    box is derived from the radius and must never be a second source of truth.
    """
    volumes = level.get("water") or []
    if not isinstance(volumes, list):
        errors.append(f"{where}: water must be an array")
        return
    if len(volumes) > MAX_LEVEL_WATER_VOLUMES:
        errors.append(
            f"{where}: too many water volumes ({len(volumes)}; limit {MAX_LEVEL_WATER_VOLUMES})"
        )
    for index, volume in enumerate(volumes):
        context = f"{where}: water volume {index}"
        if not isinstance(volume, dict):
            errors.append(f"{context} must be an object")
            continue
        shape = volume.get("shape", "rect")
        if shape not in WATER_SHAPES:
            errors.append(f"{context} shape must be one of {', '.join(WATER_SHAPES)}")
            continue
        for key in ("x", "z", "surface_y"):
            if not is_finite_number(volume.get(key)):
                errors.append(f"{context} {key} must be a finite number")
        if shape == "rect":
            for key in ("width", "depth"):
                value = volume.get(key)
                if value is None:
                    errors.append(f"{context} is a rectangle and must author {key}")
                elif not is_finite_number(value) or value <= 0.0:
                    errors.append(
                        f"{context} {key} must be a finite number greater than 0"
                    )
            if "radius" in volume:
                errors.append(
                    f"{context} authors radius on a rectangle; use \"shape\": \"circle\" "
                    "for a circular pool"
                )
        else:
            radius = volume.get("radius")
            if radius is None:
                errors.append(f"{context} is a circle and must author radius")
            elif not is_finite_number(radius) or radius <= 0.0:
                errors.append(
                    f"{context} radius must be a finite number greater than 0"
                )
            else:
                diameter = 2.0 * radius
                for key in ("width", "depth"):
                    value = volume.get(key)
                    if value is not None and (
                        not is_finite_number(value) or abs(value - diameter) > 1.0e-4
                    ):
                        errors.append(
                            f"{context} {key} must be absent or equal 2 * radius "
                            f"({diameter})"
                        )
        material = volume.get("material")
        if material is not None and (
            not isinstance(material, str) or not material.strip()
        ):
            errors.append(f"{context} material must be a non-empty id when specified")
        opacity = volume.get("opacity")
        if opacity is not None and (
            not is_finite_number(opacity) or not 0.0 <= opacity <= 1.0
        ):
            errors.append(f"{context} opacity must be a finite number between 0 and 1")
        attenuation = volume.get("attenuation_per_metre", 0.0)
        if not is_finite_number(attenuation) or not 0.0 <= attenuation <= 16.0:
            errors.append(f"{context} attenuation_per_metre must be finite in 0..16")
        bottom = volume.get("bottom_y")
        surface = volume.get("surface_y")
        if bottom is not None and (
            not is_finite_number(bottom)
            or (is_finite_number(surface) and bottom >= surface)
        ):
            errors.append(f"{context} bottom_y must be finite and below its surface_y")
        if "swimming" in volume and not isinstance(volume["swimming"], bool):
            errors.append(f"{context} swimming must be a boolean")


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
    volumes = water_footprints(level)
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
            footprint_contains_disc(footprint, x, z, radius) for footprint in volumes
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
            catalog_ids_apply = name not in GENERATED_CAPACITY_FIXTURES
            for asset_id, what in level_ids(level):
                if not catalog_ids_apply:
                    break
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
            validate_doors(level, relative, errors)
            validate_effects(level, relative, errors)
            validate_environment(level, relative, errors)
            validate_sky(level, relative, errors)
            validate_weather(level, relative, errors)
            validate_water_shapes(level, relative, errors)
            validate_floats(level, relative, errors)
            validate_surface_shine(level, relative, errors)
            validate_architecture(level, relative, errors)
            rooms = list(level.get("rooms") or [])
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
    # A generated capacity fixture is synthetic by construction (scattered
    # walls, synthetic ids, no navigation-clean layout), so its placement
    # warnings are expected and would drown the report; errors still apply.
    warnings = [
        warning
        for warning in warnings
        if not any(
            name in warning for name in GENERATED_CAPACITY_FIXTURES
        )
    ]
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

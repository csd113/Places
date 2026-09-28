#!/usr/bin/env python3
"""One-off converter: Places level sources from format v2 to format v3.

The v3 schema replaces the single-purpose ``interaction`` /
``area_triggers`` / ``manual_interaction`` authoring surface with typed
components and event bindings (see ``docs/MAP_AUTHORING_GUIDE.md`` and
``src/level.rs``). The converter applies the frozen, deterministic rules:

* ``format_version`` 2 -> 3;
* a prop's ``interaction: { prompt, reach, actions }`` becomes
  ``components: [{ "component": "interactable", prompt?, reach? }]`` and
  ``bindings: [{ "on": "interact", "actions": <actions> }]``; a prop with no
  interaction gets neither key;
* a door's ``manual_interaction`` (default ``true``) becomes an
  ``interactable`` component plus an ``on: interact`` -> ``toggle`` binding; a
  door with ``manual_interaction: false`` gets neither. The door's
  ``prompt`` (when authored) moves into the interactable component; ``reach``
  is dropped (doors use the default reach);
* ``area_triggers`` becomes ``volumes``; each volume's ``actions`` becomes one
  ``on: enter_volume`` binding, carrying an authored ``cooldown_seconds`` /
  ``once`` only when the trigger authored them;
* existing keys and their order are preserved everywhere else.

Two v3 loader coupling rules are applied as well, because the v3 schema needs
them and the v2 form could not express them:

* a ``play_animation`` / ``toggle_animation`` target must carry an
  ``animation`` component, so the first animation action's clip is hoisted
  into one (``playing: false``) when the record has none;
* ``toggle_label`` shows placed prop labels only, so an action that names a
  door is dropped with a note (v3 has no door label).

Usage:

    python3 tools/levels/convert_v3.py --all          # the maintained sources
    python3 tools/levels/convert_v3.py FILE.json ...  # explicit files
    python3 tools/levels/convert_v3.py --check --all  # report, never write

A file already at v3 needs no format conversion, but it is still checked for
the two coupling fixups above (and written only when one is missing).
``play_audio`` is never converted: if an action uses it the file is reported
and left unwritten (the action set has no audio; re-author the action
deliberately). Trailing-newline state and two-space indentation are preserved,
so a converted file diffs only where the schema changed.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Dict, List, Optional, Tuple

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
MAINTAINED = (
    "assets/levels/places_demo.json",
    "assets/levels/model_zoo.json",
    "levels/level0_pit.json",
    "levels/home_showcase.json",
    "levels/geometry_intentional.json",
)


class ConversionError(Exception):
    """A file the deterministic rules refuse to guess at."""


def _play_audio_paths(value: object, trail: str = "") -> List[str]:
    """Every JSON path whose ``action`` tag is the unsupported ``play_audio``."""
    found: List[str] = []
    if isinstance(value, dict):
        if value.get("action") == "play_audio":
            found.append(trail or "<root>")
        for key, child in value.items():
            found.extend(_play_audio_paths(child, f"{trail}.{key}" if trail else str(key)))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(_play_audio_paths(child, f"{trail}[{index}]"))
    return found


def _interactable(definition: Dict) -> Dict:
    """The ``interactable`` component for a v2 interaction/prompt/reach block."""
    component: Dict = {"component": "interactable"}
    prompt = definition.get("prompt")
    if prompt is not None:
        component["prompt"] = prompt
    reach = definition.get("reach")
    if reach is not None:
        component["reach"] = reach
    return component


def _animation_component(actions: List) -> Optional[Dict]:
    """The ``animation`` component a v3 animation action requires.

    The v3 loader only lets ``play_animation`` / ``toggle_animation`` act on a
    target that declares an ``animation`` component. The v2 interaction carried
    the clip on the action alone, so the converter hoists the first animation
    action's clip into the component (at rest, ``playing: false``, so the
    instance stays where it was until an action drives it). A chain with no
    usable clip cannot be converted and is refused.
    """
    for action in actions:
        if not isinstance(action, dict):
            continue
        tag = action.get("action")
        if tag not in ("play_animation", "toggle_animation"):
            continue
        clip = action.get("clip")
        if not isinstance(clip, str) or not clip.strip():
            return None
        return {
            "component": "animation",
            "clip": clip.strip(),
            "looped": bool(action.get("loop", False)) if tag == "play_animation" else False,
            "playing": False,
        }
    return None


def _interaction_components(interaction: Dict) -> List[Dict]:
    """The v3 components one v2 interaction block becomes."""
    components = [_interactable(interaction)]
    animation = _animation_component(interaction.get("actions"))
    if animation is not None:
        components.append(animation)
    return components


def _has_animation_component(components: object) -> bool:
    return isinstance(components, list) and any(
        isinstance(component, dict) and component.get("component") == "animation"
        for component in components
    )


def _actions_need_animation(actions: object) -> bool:
    return isinstance(actions, list) and any(
        isinstance(action, dict)
        and action.get("action") in ("play_animation", "toggle_animation")
        for action in actions
    )


def _drop_door_labels(actions: List, door_ids: set, where: str, report: List[str]) -> List:
    """v3 labels live on placed props, so a door-targeted toggle_label is dropped."""
    kept: List = []
    for action in actions:
        if (
            isinstance(action, dict)
            and action.get("action") == "toggle_label"
            and isinstance(action.get("target"), str)
            and action["target"].strip() in door_ids
        ):
            report.append(
                f"{where}: dropped toggle_label on door '{action['target'].strip()}' "
                "(v3 shows labels only on placed props)"
            )
            continue
        kept.append(action)
    return kept


def _convert_prop(prop: Dict, where: str) -> Dict:
    interaction = prop.get("interaction")
    if interaction is None:
        return prop
    if not isinstance(interaction, dict):
        raise ConversionError(f"{where}: prop interaction must be an object")
    actions = interaction.get("actions")
    if not isinstance(actions, list) or not actions:
        raise ConversionError(f"{where}: prop interaction declares no actions")
    if _actions_need_animation(actions) and _animation_component(actions) is None:
        raise ConversionError(
            f"{where}: an animation action names no clip; author the prop's animation "
            "component explicitly before converting"
        )
    converted: Dict = {}
    for key, value in prop.items():
        if key != "interaction":
            converted[key] = value
            continue
        converted["components"] = _interaction_components(interaction)
        converted["bindings"] = [{"on": "interact", "actions": actions}]
    return converted


def _convert_door(door: Dict, where: str) -> Dict:
    manual = door.get("manual_interaction", True)
    if not isinstance(manual, bool):
        raise ConversionError(f"{where}: manual_interaction must be a boolean")
    interactive = manual and "components" not in door and "bindings" not in door
    # A manually interactable door's prompt is phase-dependent by default; an
    # authored prompt moves into the interactable component, an authored reach
    # is dropped (the leaf keeps the engine default).
    components = [_interactable({"prompt": door.get("prompt")})]
    bindings = [{"on": "interact", "actions": [{"action": "toggle"}]}]
    converted: Dict = {}
    inserted = False
    for key, value in door.items():
        if key in ("manual_interaction", "prompt", "reach"):
            if interactive and not inserted and key == "manual_interaction":
                converted["components"] = components
                converted["bindings"] = bindings
                inserted = True
            continue
        converted[key] = value
    if interactive and not inserted:
        converted["components"] = components
        converted["bindings"] = bindings
    return converted


def _convert_trigger(trigger: Dict, where: str) -> Dict:
    actions = trigger.get("actions")
    if not isinstance(actions, list) or not actions:
        raise ConversionError(f"{where}: area trigger declares no actions")
    binding: Dict = {"on": "enter_volume", "actions": actions}
    if "cooldown_seconds" in trigger:
        binding["cooldown_seconds"] = trigger["cooldown_seconds"]
    if "once" in trigger:
        binding["once"] = trigger["once"]
    converted: Dict = {}
    for key, value in trigger.items():
        if key == "actions":
            converted["bindings"] = [binding]
        elif key in ("cooldown_seconds", "once"):
            continue
        else:
            converted[key] = value
    return converted


_BINDING_CARRIERS = (
    "props",
    "doors",
    "ceiling_lights",
    "effects",
    "volumes",
    "timers",
    "spawn_points",
    "spawn_templates",
)
_COMPONENT_CARRIERS = ("props", "doors", "spawn_templates")


def _apply_v3_fixups(level: Dict, where: str, report: List[str]) -> bool:
    """Applies the v3 loader's two implicit coupling rules; returns changed.

    * a target of ``play_animation`` / ``toggle_animation`` must carry an
      ``animation`` component, so the first animation action's clip is hoisted
      into one when the record has none;
    * ``toggle_label`` shows placed prop labels only, so an action that names a
      door is dropped (it cannot be expressed in v3).
    """
    door_ids = {
        str(door.get("id")).strip()
        for door in level.get("doors") or []
        if isinstance(door, dict) and isinstance(door.get("id"), str)
    }
    changed = False
    for key in _BINDING_CARRIERS:
        for index, record in enumerate(level.get(key) or []):
            if not isinstance(record, dict):
                continue
            bindings = record.get("bindings")
            if not isinstance(bindings, list):
                continue
            for position, binding in enumerate(bindings):
                if not isinstance(binding, dict):
                    continue
                actions = binding.get("actions")
                if not isinstance(actions, list):
                    continue
                pruned = _drop_door_labels(
                    actions, door_ids, f"{where}: {key} {index} binding {position}", report
                )
                if len(pruned) != len(actions):
                    if not pruned:
                        raise ConversionError(
                            f"{where}: {key} {index} binding {position} is left with no actions"
                        )
                    binding["actions"] = pruned
                    changed = True
                if key not in _COMPONENT_CARRIERS:
                    continue
                if _actions_need_animation(pruned) and not _has_animation_component(
                    record.get("components")
                ):
                    animation = _animation_component(pruned)
                    if animation is None:
                        raise ConversionError(
                            f"{where}: {key} {index} uses an animation action that names no "
                            "clip; author its `animation` component explicitly"
                        )
                    components = record.setdefault("components", [])
                    if not isinstance(components, list):
                        raise ConversionError(f"{where}: {key} {index} components must be an array")
                    components.append(animation)
                    report.append(
                        f"{where}: {key} {index} gained an `animation` component "
                        f"(clip '{animation['clip']}') required by its animation action"
                    )
                    changed = True
    return changed


def convert_level(level: Dict, where: str) -> Tuple[Dict, bool, List[str]]:
    """Returns the converted level, whether anything changed, and the report."""
    if not isinstance(level, dict):
        raise ConversionError(f"{where}: the level document must be an object")
    play_audio = _play_audio_paths(level)
    if play_audio:
        raise ConversionError(
            f"{where}: play_audio is unsupported and is not converted "
            f"(found at {', '.join(play_audio)})"
        )
    version = level.get("format_version")
    if version not in (2, 3):
        raise ConversionError(f"{where}: expected format_version 2 or 3, found {version!r}")

    report: List[str] = []
    if version == 2:
        converted: Dict = {}
        for key, value in level.items():
            if key == "format_version":
                converted[key] = 3
            elif key == "area_triggers":
                if not isinstance(value, list):
                    raise ConversionError(f"{where}: area_triggers must be an array")
                converted["volumes"] = [
                    _convert_trigger(trigger, f"{where}: area trigger {index}")
                    for index, trigger in enumerate(value)
                ]
            elif key == "props":
                if not isinstance(value, list):
                    raise ConversionError(f"{where}: props must be an array")
                converted[key] = [
                    _convert_prop(prop, f"{where}: prop {index}")
                    for index, prop in enumerate(value)
                ]
            elif key == "doors":
                if not isinstance(value, list):
                    raise ConversionError(f"{where}: doors must be an array")
                converted[key] = [
                    _convert_door(door, f"{where}: door {index}")
                    for index, door in enumerate(value)
                ]
            else:
                converted[key] = value
    else:
        converted = level
    # The fixups run on both paths: a fresh v2 conversion already hoisted the
    # animation components, but the door-label pruning applies to it too.
    changed = version == 2
    if _apply_v3_fixups(converted, where, report):
        changed = True
    return converted, changed, report


def _dump(level: Dict, trailing_newline: bool) -> bytes:
    rendered = json.dumps(level, indent=2, ensure_ascii=False)
    if trailing_newline:
        rendered += "\n"
    return rendered.encode("utf-8")


def convert_file(path: str, check_only: bool) -> int:
    """Converts one file; returns 0 on success, 1 on a refusal/failure."""
    display = os.path.relpath(path, APP_ROOT)
    try:
        raw = open(path, "rb").read()
    except OSError as error:
        print(f"[convert_v3] cannot read {display}: {error}")
        return 1
    try:
        level = json.loads(raw)
    except json.JSONDecodeError as error:
        print(f"[convert_v3] {display}: not valid JSON: {error}")
        return 1
    try:
        converted, changed, report = convert_level(level, display)
    except ConversionError as error:
        print(f"[convert_v3] REFUSED {error}")
        return 1
    for line in report:
        print(f"[convert_v3] note {line}")
    if not changed:
        print(f"[convert_v3] ok {display} (format_version 3 and loader fixups applied)")
        return 0
    if check_only:
        print(f"[convert_v3] STALE {display}: needs the format_version 3 conversion or fixups")
        return 1
    rendered = _dump(converted, trailing_newline=raw.endswith(b"\n"))
    with open(path, "wb") as handle:
        handle.write(rendered)
    print(f"[convert_v3] converted {display} -> format_version 3")
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("files", nargs="*", help="level JSON paths to convert")
    parser.add_argument("--all", action="store_true", help="convert the five maintained sources")
    parser.add_argument("--check", action="store_true", help="report stale files without writing")
    args = parser.parse_args(argv)

    paths: List[str] = [os.path.abspath(path) for path in args.files]
    if args.all:
        paths.extend(os.path.join(APP_ROOT, path) for path in MAINTAINED)
    if not paths:
        parser.error("pass level JSON paths or --all")
    failures = 0
    for path in paths:
        failures += convert_file(path, args.check)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

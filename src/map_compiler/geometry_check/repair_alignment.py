#!/usr/bin/env python3
"""Check a read-only Places geometry plan against its current saved source.

Generate the plan with the native command:
    places --repair-geometry --level path/to/level.json --plan plan.json

Verify the source hash and old values without changing any file:
    python3 src/map_compiler/geometry_check/repair_alignment.py --plan plan.json --check

Use the report to make and validate the changes manually in places-editor.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from typing import Any, List, Optional, Tuple

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
PLAN_FORMAT = "places-geometry-repair-plan"
PLAN_VERSION = 1
# f32 shortest-repr noise only: a real authored change is at least 1e-4 m.
FLOAT_TOLERANCE = 1.0e-6


class Refused(Exception):
    """A verification failure that must leave the source untouched."""


def load_plan(path: str) -> dict:
    with open(path, "rb") as handle:
        plan = json.loads(handle.read())
    if not isinstance(plan, dict) or plan.get("format") != PLAN_FORMAT:
        raise Refused(f"{path}: not a {PLAN_FORMAT} document")
    if plan.get("version") != PLAN_VERSION:
        raise Refused(f"{path}: unsupported plan version {plan.get('version')!r}")
    level = plan.get("level")
    if not isinstance(level, dict) or not isinstance(level.get("source"), str):
        raise Refused(f"{path}: the plan names no level source")
    if not isinstance(plan.get("edits"), list):
        raise Refused(f"{path}: the plan has no edits array")
    return plan


def resolve_source(plan_path: str, source: str) -> str:
    if os.path.isabs(source):
        return source
    for base in (APP_ROOT, os.getcwd(), os.path.dirname(os.path.abspath(plan_path))):
        candidate = os.path.join(base, source)
        if os.path.isfile(candidate):
            return candidate
    return os.path.join(APP_ROOT, source)


def pointer_tokens(pointer: str) -> List[str]:
    if not pointer.startswith("/"):
        raise Refused(f"bad JSON pointer {pointer!r}: must start with '/'")
    tokens = []
    for token in pointer[1:].split("/"):
        tokens.append(token.replace("~1", "/").replace("~0", "~"))
    return tokens


def resolve_pointer(document: Any, pointer: str) -> Any:
    current = document
    for token in pointer_tokens(pointer):
        if isinstance(current, dict):
            if token not in current:
                raise Refused(f"pointer {pointer!r}: key {token!r} is missing")
            current = current[token]
        elif isinstance(current, list):
            try:
                index = int(token)
            except ValueError as error:
                raise Refused(f"pointer {pointer!r}: {token!r} is not a list index") from error
            if index < 0 or index >= len(current):
                raise Refused(f"pointer {pointer!r}: index {index} is out of range")
            current = current[index]
        else:
            raise Refused(f"pointer {pointer!r}: {token!r} addresses a scalar")
    return current


def values_match(current: Any, expected: Any) -> bool:
    if isinstance(current, bool) or isinstance(expected, bool):
        return current is expected
    if isinstance(current, (int, float)) and isinstance(expected, (int, float)):
        return abs(float(current) - float(expected)) <= FLOAT_TOLERANCE
    return current == expected


def edit_target_value(edit: dict) -> Tuple[str, Any, Any]:
    pointer = edit.get("pointer")
    if not isinstance(pointer, str):
        raise Refused("an edit has no string `pointer`")
    if "old" not in edit or "new" not in edit:
        raise Refused(f"edit {pointer}: `old` and `new` are required")
    return pointer, edit["old"], edit["new"]


def verify_plan(plan: dict, raw: bytes, document: Any) -> List[str]:
    """Checks the hash and every edit's current value; returns the reasons."""
    reasons: List[str] = []
    expected_sha = plan["level"].get("sha256")
    actual_sha = hashlib.sha256(raw).hexdigest()
    if expected_sha != actual_sha:
        reasons.append(
            f"source sha256 {actual_sha} does not match the plan's {expected_sha} "
            "(a concurrent change was refused)"
        )
    for position, edit in enumerate(plan["edits"]):
        if not isinstance(edit, dict):
            reasons.append(f"edit {position} is not an object")
            continue
        try:
            pointer, old, _new = edit_target_value(edit)
            current = resolve_pointer(document, pointer)
        except Refused as error:
            reasons.append(f"edit {position} ({edit.get('pointer')!r}): {error}")
            continue
        if not values_match(current, old):
            reasons.append(
                f"edit {position} {pointer}: the source has {current!r}, the plan expects "
                f"{old!r}"
            )
    return reasons


def command_check(plan_path: str, plan: dict) -> int:
    source = resolve_source(plan_path, plan["level"]["source"])
    try:
        with open(source, "rb") as handle:
            raw = handle.read()
        document = json.loads(raw)
    except OSError as error:
        print(f"[repair-alignment] cannot read {source}: {error}")
        return 1
    except json.JSONDecodeError as error:
        print(f"[repair-alignment] {source}: not valid JSON: {error}")
        return 1
    reasons = verify_plan(plan, raw, document)
    if reasons:
        for reason in reasons:
            print(f"[repair-alignment] REFUSED {reason}")
        return 1
    print(
        f"[repair-alignment] ok {os.path.relpath(source, APP_ROOT)}: "
        f"{len(plan['edits'])} edit(s) verified, nothing written"
    )
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--plan", required=True, help="machine-readable repair plan")
    parser.add_argument("--check", action="store_true", required=True,
                        help="verify the plan, never write")
    args = parser.parse_args(argv)

    try:
        plan = load_plan(args.plan)
    except (Refused, OSError, json.JSONDecodeError) as error:
        print(f"[repair-alignment] REFUSED {error}")
        return 1
    return command_check(args.plan, plan)


if __name__ == "__main__":
    sys.exit(main())

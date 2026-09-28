#!/usr/bin/env python3
"""Apply a `places --repair-geometry` plan to a level source, deterministically.

The Rust planner only reads sources; this script is the map-side applier for
its plans (the same convention as ``tools/levels/convert_v3.py``: order
preserving JSON, two-space indent, the source's trailing-newline state).

Both modes verify the plan against the current bytes before doing anything:

* the source's SHA-256 must match ``plan.level.sha256`` (a concurrent edit is
  refused, not merged);
* every edit's JSON pointer must resolve to the plan's ``old`` value.

``--check`` stops after the verification and never writes. ``--apply``:

1. applies every edit to the parsed document, preserving key order;
2. writes the result to a temporary sibling and re-runs
   ``places --check-geometry`` on it, requiring exit 0;
3. ``os.replace``s it over the source (atomic);
4. re-runs ``places --repair-geometry`` and requires an empty second plan
   (idempotence);
5. on any failure, restores the original bytes.

No temporary file is ever left behind, and the maintained sources are only
written by an explicit ``--apply`` whose post-checks all pass.

Usage:

    python3 tools/levels/repair_alignment.py --plan plan.json --check
    python3 tools/levels/repair_alignment.py --plan plan.json --apply

Set ``PLACES_BIN`` to choose the ``places`` executable; the default is
``target/release/places``, falling back to ``target/debug/places``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from typing import Any, List, Optional, Tuple

APP_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
PLAN_FORMAT = "places-geometry-repair-plan"
PLAN_VERSION = 1
# f32 shortest-repr noise only: a real authored change is at least 1e-4 m.
FLOAT_TOLERANCE = 1.0e-6


class Refused(Exception):
    """A verification failure that must leave the source untouched."""


def places_binary() -> str:
    configured = os.environ.get("PLACES_BIN")
    if configured:
        return configured
    for candidate in ("target/release/places", "target/debug/places"):
        path = os.path.join(APP_ROOT, candidate)
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    raise Refused(
        "no places executable found; set PLACES_BIN or build target/release/places"
    )


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


def assign_pointer(document: Any, pointer: str, value: Any) -> None:
    tokens = pointer_tokens(pointer)
    if not tokens:
        raise Refused(f"pointer {pointer!r} addresses the root")
    parent = document
    for token in tokens[:-1]:
        if isinstance(parent, dict):
            if token not in parent:
                raise Refused(f"pointer {pointer!r}: key {token!r} is missing")
            parent = parent[token]
        elif isinstance(parent, list):
            try:
                index = int(token)
            except ValueError as error:
                raise Refused(f"pointer {pointer!r}: {token!r} is not a list index") from error
            if index < 0 or index >= len(parent):
                raise Refused(f"pointer {pointer!r}: index {index} is out of range")
            parent = parent[index]
        else:
            raise Refused(f"pointer {pointer!r}: {token!r} addresses a scalar")
    last = tokens[-1]
    if isinstance(parent, dict):
        if last not in parent:
            raise Refused(f"pointer {pointer!r}: key {last!r} is missing")
        parent[last] = value
    elif isinstance(parent, list):
        try:
            index = int(last)
        except ValueError as error:
            raise Refused(f"pointer {pointer!r}: {last!r} is not a list index") from error
        if index < 0 or index >= len(parent):
            raise Refused(f"pointer {pointer!r}: index {index} is out of range")
        parent[index] = value
    else:
        raise Refused(f"pointer {pointer!r}: {last!r} addresses a scalar")


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


def serialise(document: Any, trailing_newline: bool) -> bytes:
    rendered = json.dumps(document, indent=2, ensure_ascii=False)
    if trailing_newline:
        rendered += "\n"
    return rendered.encode("utf-8")


def write_atomic(path: str, payload: bytes) -> None:
    directory = os.path.dirname(os.path.abspath(path)) or "."
    temp = os.path.join(
        directory, f".{os.path.basename(path)}.places-repair-{os.getpid()}.json"
    )
    try:
        with open(temp, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def run_checker(binary: str, level_path: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [binary, "--check-geometry", "--level", level_path, "--quiet"],
        capture_output=True,
        text=True,
        cwd=APP_ROOT,
        check=False,
    )


def run_planner(binary: str, level_path: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [binary, "--repair-geometry", "--level", level_path, "--json"],
        capture_output=True,
        text=True,
        cwd=APP_ROOT,
        check=False,
    )


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


def command_apply(plan_path: str, plan: dict) -> int:
    binary = places_binary()
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
    if not plan["edits"]:
        print(f"[repair-alignment] ok {os.path.relpath(source, APP_ROOT)}: the plan has no edits")
        return 0

    try:
        for edit in plan["edits"]:
            pointer, _old, new = edit_target_value(edit)
            assign_pointer(document, pointer, new)
    except Refused as error:
        print(f"[repair-alignment] REFUSED {error}")
        return 1
    rendered = serialise(document, raw.endswith(b"\n"))

    directory = os.path.dirname(os.path.abspath(source)) or "."
    temp = os.path.join(
        directory, f".{os.path.basename(source)}.places-repair-{os.getpid()}.json"
    )
    replaced = False
    try:
        with open(temp, "wb") as handle:
            handle.write(rendered)
            handle.flush()
            os.fsync(handle.fileno())
        checked = run_checker(binary, temp)
        if checked.returncode != 0:
            print(
                f"[repair-alignment] REFUSED the edited map does not pass "
                f"--check-geometry (exit {checked.returncode}):\n{checked.stdout}{checked.stderr}"
            )
            return 1
        os.replace(temp, source)
        replaced = True
        second = run_planner(binary, source)
        if second.returncode != 0:
            print(
                f"[repair-alignment] FAILED the second plan is not empty "
                f"(exit {second.returncode}):\n{second.stdout}{second.stderr}"
            )
            write_atomic(source, raw)
            print("[repair-alignment] restored the original bytes")
            return 1
        try:
            second_plan = json.loads(second.stdout)
        except json.JSONDecodeError as error:
            print(f"[repair-alignment] FAILED the second plan is not JSON: {error}")
            write_atomic(source, raw)
            print("[repair-alignment] restored the original bytes")
            return 1
        if second_plan.get("edits") or second_plan.get("findings"):
            print(
                "[repair-alignment] FAILED the second plan is not empty: "
                f"{len(second_plan.get('edits', []))} edit(s), "
                f"{len(second_plan.get('findings', []))} finding(s)"
            )
            write_atomic(source, raw)
            print("[repair-alignment] restored the original bytes")
            return 1
    except OSError as error:
        print(f"[repair-alignment] FAILED {error}")
        if replaced:
            write_atomic(source, raw)
            print("[repair-alignment] restored the original bytes")
        return 1
    finally:
        if os.path.exists(temp):
            os.unlink(temp)

    print(
        f"[repair-alignment] applied {len(plan['edits'])} edit(s) to "
        f"{os.path.relpath(source, APP_ROOT)}; the second plan is empty"
    )
    return 0


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--plan", required=True, help="machine-readable repair plan")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="verify the plan, never write")
    mode.add_argument("--apply", action="store_true", help="apply the plan atomically")
    args = parser.parse_args(argv)

    try:
        plan = load_plan(args.plan)
    except (Refused, OSError, json.JSONDecodeError) as error:
        print(f"[repair-alignment] REFUSED {error}")
        return 1
    if args.check:
        return command_check(args.plan, plan)
    return command_apply(args.plan, plan)


if __name__ == "__main__":
    sys.exit(main())

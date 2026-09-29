#!/usr/bin/env python3
"""Scatter alpha-cutout grass tufts over level areas (deterministic).

A level-authoring helper, not a runtime system: it derives ``props[]``
entries for the two outdoor grass tufts and either prints them, writes them
to a file, or merges them into a level file.  The game's quality tiers are
untouched by this tool -- it only proposes prop placements, and the level
validator (`tools/assets/validate.py`) vets whatever a level ends up with.

Density profiles (instances per square metre):

======  ============  ==============  ==========================
profile  instances/m2  min spacing m  model mix (small : large)
======  ============  ==============  ==========================
low      0.5           0.45            2 : 1
medium   1.4           0.45            1 : 1
dense    3.0           0.30            1 : 2
======  ============  ==============  ==========================

Placement: an area is tiled at ``1/sqrt(density)`` metre cells, one candidate
per cell jittered inside it, then every candidate is filtered against a global
minimum spacing (a spatial hash) and against the keep-out rectangles.  Keep-out
rectangles are inclusive: a tuft centre exactly on a keep-out boundary is
rejected.

Seed guarantee: candidates are generated from a tiny seeded LCG (the same
multiply-add stream as ``tools/levels/build_capacity_fixtures.py``), areas and
keep-outs are normalised and sorted before use, the surviving points are sorted
by ``(z, x)``, and only then are ids, model choices, rotations and scales drawn.
The same seed, areas, keep-outs and model list therefore always produce
byte-identical JSON, whatever order the options are given in.

Emitted props are exactly the ``PropDef`` fields the game reads for a placed
tuft -- ``id``, ``model``, ``x``, ``z``, ``rotation_degrees``, ``scale`` and
``occludes`` -- and nothing else.  ``occludes`` is always false: alpha-cutout
blades must not grind solid shadow boxes out of the baked lighting.

Usage::

    python3 tools/levels/scatter_grass.py --area 4,4,20,12 --density low
    python3 tools/levels/scatter_grass.py --area 4,4,20,12 --density dense \\
        --keep-out 8,6,4,4 --seed 7 --out target/grass.json
    python3 tools/levels/scatter_grass.py --area 4,4,20,12 --density medium \\
        --target assets/levels/yard.json --apply
    python3 tools/levels/scatter_grass.py --area 4,4,20,12 --density medium \\
        --target assets/levels/yard.json --check
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
import tempfile
from typing import Dict, List, Optional, Sequence, Tuple

# --------------------------------------------------------------------------
# Profiles
# --------------------------------------------------------------------------

DENSITY = {"low": 0.5, "medium": 1.4, "dense": 3.0}
MIN_SPACING = {"low": 0.45, "medium": 0.45, "dense": 0.30}
# First-model ("small") probability, held as an integer ratio so the draw is
# exact rather than a float comparison: 2-in-3 at low, 1-in-2 at medium (the
# special value 3 below selects the even/odd branch), 1-in-3 at dense.
SMALL_OUT_OF_THREE = {"low": 2, "medium": 3, "dense": 1}
DEFAULT_MODELS = "outdoor:grass_patch_small,outdoor:grass_patch_large"

Rect = Tuple[float, float, float, float]  # x, z, width, depth (positive)


class Rng:
    """Tiny deterministic LCG, matching build_capacity_fixtures.py."""

    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFF_FFFF

    def next(self) -> int:
        self.state = (self.state * 1_664_525 + 1_013_904_223) & 0xFFFF_FFFF
        return self.state

    def unit(self) -> float:
        return self.next() / 0xFFFF_FFFF

    def between(self, low: float, high: float) -> float:
        return low + (high - low) * self.unit()


# --------------------------------------------------------------------------
# Geometry helpers
# --------------------------------------------------------------------------


def _normalise(rect: Sequence[float], what: str) -> Rect:
    """(x, z, width, depth) with a positive extent, else SystemExit."""
    x, z, width, depth = (float(value) for value in rect)
    if width < 0.0:
        x += width
        width = -width
    if depth < 0.0:
        z += depth
        depth = -depth
    if width <= 0.0 or depth <= 0.0:
        raise SystemExit(f"{what}: width and depth must be positive (got {width} x {depth})")
    return (x, z, width, depth)


def _inside(x: float, z: float, rect: Rect) -> bool:
    rx, rz, width, depth = rect
    return rx <= x <= rx + width and rz <= z <= rz + depth


class Spacing:
    """Spatial hash that keeps every accepted point ``minimum`` apart."""

    def __init__(self, minimum: float) -> None:
        self.minimum = minimum
        self.cell = minimum
        self.kept: Dict[Tuple[int, int], List[Tuple[float, float]]] = {}

    def accepts(self, x: float, z: float) -> bool:
        cell_x = math.floor(x / self.cell)
        cell_z = math.floor(z / self.cell)
        limit = self.minimum * self.minimum
        for offset_x in (-1, 0, 1):
            for offset_z in (-1, 0, 1):
                for other_x, other_z in self.kept.get((cell_x + offset_x, cell_z + offset_z), ()):
                    if (x - other_x) ** 2 + (z - other_z) ** 2 < limit:
                        return False
        self.kept.setdefault((cell_x, cell_z), []).append((x, z))
        return True


def _candidates(rng: Rng, areas: Sequence[Rect], density: float, spacing: float):
    """Jittered grid candidates over every area, in sorted-area order."""
    step = 1.0 / math.sqrt(density)
    for (area_x, area_z, width, depth) in areas:
        columns = max(1, int(round(width / step)))
        rows = max(1, int(round(depth / step)))
        cell_x = width / columns
        cell_z = depth / rows
        # Per-axis jitter bounds keep two neighbours of the same grid at least
        # ``spacing`` apart even when both jitter straight at each other.
        jitter_x = max(0.0, (cell_x - spacing) * 0.5)
        jitter_z = max(0.0, (cell_z - spacing) * 0.5)
        for row in range(rows):
            for column in range(columns):
                x = area_x + cell_x * (column + 0.5) + rng.between(-jitter_x, jitter_x)
                z = area_z + cell_z * (row + 0.5) + rng.between(-jitter_z, jitter_z)
                yield x, z


def scatter(
    areas: Sequence[Rect],
    keep_outs: Sequence[Rect],
    density: str,
    seed: int,
    models: Sequence[str],
    id_prefix: str = "grass_",
) -> List[dict]:
    """Derives the deterministic prop list for one scatter request.

    ``id_prefix`` namespaces one band's ids (``grass_`` by default, or
    ``grass_low_`` when several densities share a level); it must start with
    ``grass_`` so the level-wide replace/check filters keep matching every band.
    """
    if not id_prefix.startswith("grass_"):
        raise ValueError("id_prefix must start with 'grass_'")
    rng = Rng(seed)
    minimum = MIN_SPACING[density]
    # Round first, then filter on the emitted coordinates, so the JSON really
    # is ordered by the (z, x) the level reads -- and both the spacing gate and
    # the keep-out/containment checks are measured on exactly those numbers.
    points: List[Tuple[float, float]] = []
    for (x, z) in _candidates(rng, areas, DENSITY[density], minimum):
        x = round(x, 3)
        z = round(z, 3)
        if x == 0:
            x = 0.0
        if z == 0:
            z = 0.0
        if not any(_inside(x, z, area) for area in areas):
            continue
        if any(_inside(x, z, rect) for rect in keep_outs):
            continue
        points.append((x, z))
    points.sort(key=lambda point: (point[1], point[0]))

    spacing = Spacing(minimum)
    kept = [(x, z) for (x, z) in points if spacing.accepts(x, z)]

    small_out_of_three = SMALL_OUT_OF_THREE[density]
    props: List[dict] = []
    for index, (x, z) in enumerate(kept, start=1):
        if len(models) == 1:
            model = models[0]
        elif small_out_of_three == 3:
            model = models[0] if rng.next() % 2 == 0 else models[1]
        else:
            model = models[0] if rng.next() % 3 < small_out_of_three else models[1]
        rotation = rng.between(0.0, 359.9)
        scale = rng.between(0.85, 1.15)
        props.append(
            {
                "id": f"{id_prefix}{index}",
                "model": model,
                "x": x,
                "z": z,
                "rotation_degrees": round(rotation, 1),
                "scale": round(scale, 3),
                "occludes": False,
            }
        )
    return props


def render_json(props: Sequence[dict]) -> str:
    """The emitted props array: pretty, 2-space, trailing newline."""
    return json.dumps(list(props), indent=2) + "\n"


def render_level(level: dict) -> str:
    """A level document round-tripped through the same pretty format."""
    return json.dumps(level, indent=2) + "\n"


def _atomic_write(path: str, payload: str) -> None:
    directory = os.path.dirname(os.path.abspath(path))
    os.makedirs(directory, exist_ok=True)
    handle = tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", delete=False, dir=directory, prefix=".scatter_grass."
    )
    try:
        handle.write(payload)
        handle.close()
        os.replace(handle.name, path)
    except BaseException:
        handle.close()
        if os.path.exists(handle.name):
            os.unlink(handle.name)
        raise


def _read_level(path: str) -> dict:
    try:
        with open(path, "r", encoding="utf-8") as handle:
            level = json.load(handle)
    except OSError as error:
        raise SystemExit(f"cannot read target level {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path} is not valid JSON: {error}") from error
    if not isinstance(level, dict):
        raise SystemExit(f"{path}: the level root is not an object")
    return level


def _grass_props(level: dict) -> List[dict]:
    props = level.get("props")
    if not isinstance(props, list):
        return []
    return [prop for prop in props if isinstance(prop, dict) and str(prop.get("id", "")).startswith("grass_")]


def _describe_difference(expected: Sequence[dict], actual: Sequence[dict]) -> str:
    lines = [f"the level holds {len(actual)} grass prop(s), the tool derives {len(expected)}"]
    for index in range(max(len(expected), len(actual))):
        want = expected[index] if index < len(expected) else None
        have = actual[index] if index < len(actual) else None
        if want != have:
            lines.append(f"  [grass_{index + 1}] level={have}")
            lines.append(f"               derived={want}")
            if len(lines) > 13:
                lines.append("  ...")
                break
    return "\n".join(lines)


# --------------------------------------------------------------------------


def _rect_arg(text: str) -> List[float]:
    """``X,Z,W,D`` (commas or spaces) as four floats for one --area/--keep-out."""
    parts = text.replace(",", " ").split()
    try:
        values = [float(part) for part in parts]
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"'{text}' is not X,Z,W,D") from error
    if len(values) != 4:
        raise argparse.ArgumentTypeError(
            f"'{text}' must be X,Z,W,D (four numbers, got {len(values)})"
        )
    return values


def _fix_negative_rects(argv: List[str]) -> List[str]:
    """Re-spells ``--area -6,-2,8,10`` as ``--area=-6,-2,8,10``.

    argparse treats a comma-separated negative coordinate token as a possible
    option flag, so a leading minus is otherwise rejected even though the value
    is attached to the rectangle option.
    """
    fixed: List[str] = []
    index = 0
    while index < len(argv):
        token = argv[index]
        if token in ("--area", "--keep-out") and index + 1 < len(argv):
            value = argv[index + 1]
            if value.startswith("-") and not value.startswith("--"):
                fixed.append(f"{token}={value}")
                index += 2
                continue
        fixed.append(token)
        index += 1
    return fixed


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--area", type=_rect_arg, action="append", required=True,
        metavar="X,Z,W,D",
        help="scatter area as X,Z,W,D (repeatable; may overlap)",
    )
    parser.add_argument("--density", choices=sorted(DENSITY), required=True, help="instances per square metre profile")
    parser.add_argument("--seed", type=int, default=1, help="deterministic LCG seed (default 1)")
    parser.add_argument(
        "--keep-out", type=_rect_arg, action="append", default=[],
        metavar="X,Z,W,D",
        help="rectangle no grass centre may fall inside (repeatable)",
    )
    parser.add_argument(
        "--models", default=DEFAULT_MODELS,
        help="one or two comma-separated model ids (default: the two grass tufts)",
    )
    parser.add_argument(
        "--id-prefix", default="grass_", metavar="PREFIX",
        help="instance-id prefix; must start with 'grass_' (default grass_)",
    )
    parser.add_argument("--target", metavar="LEVEL.JSON", help="level file to merge into or check")
    parser.add_argument("--out", metavar="FILE", help="write the derived JSON array to FILE")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--apply", action="store_true", help="replace the level's grass props with the derived set")
    mode.add_argument("--check", action="store_true", help="fail when the level's grass props differ from the derived set")
    args = parser.parse_args(_fix_negative_rects(list(sys.argv[1:] if argv is None else argv)))

    if args.target and not (args.apply or args.check):
        parser.error("--target requires --apply or --check")
    if (args.apply or args.check) and not args.target:
        parser.error("--apply/--check require --target")

    models = [model.strip() for model in args.models.split(",") if model.strip()]
    if not 1 <= len(models) <= 2:
        parser.error("--models takes one or two comma-separated ids")
    if not args.id_prefix.startswith("grass_"):
        parser.error("--id-prefix must start with 'grass_'")

    # Normalise and sort the geometry: the same rectangles in any option order
    # produce the same RNG stream, keep-out filter and output ordering.
    areas = sorted(
        _normalise(value, "--area") for value in args.area
    )
    keep_outs = sorted(
        _normalise(value, "--keep-out") for value in args.keep_out
    )

    props = scatter(areas, keep_outs, args.density, args.seed, models, args.id_prefix)
    payload = render_json(props)

    if args.out:
        _atomic_write(args.out, payload)
        print(f"wrote {args.out} ({len(props)} grass prop(s), density {args.density}, seed {args.seed})",
              file=sys.stderr)

    if args.check:
        level = _read_level(args.target)
        existing = _grass_props(level)
        if existing == props:
            print(f"ok {args.target}: {len(props)} grass prop(s) match the derived scatter")
            return 0
        print(f"stale {args.target}:")
        print(_describe_difference(props, existing))
        return 1

    if args.apply:
        level = _read_level(args.target)
        all_props = level.get("props")
        if not isinstance(all_props, list):
            all_props = []
        kept = [
            prop for prop in all_props
            if not (isinstance(prop, dict) and str(prop.get("id", "")).startswith("grass_"))
        ]
        level["props"] = kept + props
        _atomic_write(args.target, render_level(level))
        print(f"applied {len(props)} grass prop(s) to {args.target} (density {args.density}, seed {args.seed})")

    if not args.out and not args.apply and not args.check:
        sys.stdout.write(payload)
    return 0


if __name__ == "__main__":
    sys.exit(main())

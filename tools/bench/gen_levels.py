#!/usr/bin/env python3
"""Generate deterministic prop-stress levels for the PocketCHIP benchmarks.

The levels are plain `LevelDef` JSON, exactly what a community author would
ship, so nothing about the benchmark bypasses the normal load path: the game
discovers them in `levels/`, bakes their lighting and batches their props in the
usual way.

Layout is chosen so one level serves both benchmark shapes:

* a hall sized to the chair grid, with three ceiling fixtures on the centre line;
* `N` chairs on a fixed square grid centred on the hall origin;
* the spawn sits just inside the -Z wall, so looking along +Z points the camera
  at every chair, and looking along -Z points it at the near wall with the whole
  grid *behind* the camera. That is the camera-away test.

The generator is pure: same arguments, byte-identical output.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

# Chair grid spacing. Chairs are 0.5 m wide; 1.4 m keeps them visually separate
# while still packing a dense field into the hall.
CHAIR_SPACING = 1.4
# Clearance between the outermost chair and the inside face of the wall.
ROOM_MARGIN = 6.0
ROOM_HEIGHT = 3.0
WALL_THICKNESS = 0.4

DEFAULTS = {
    "wall": "core:wallpaper_yellow_01",
    "floor": "core:carpet_beige_01",
    "ceiling": "core:ceiling_panel_01",
}

# Chairs are rotated through a fixed set of yaws so the field never looks like a
# perfectly regular lattice. Deterministic, and irrelevant to vertex cost.
YAW_PATTERN = (0.0, 37.0, 111.0, 203.0, 289.0, 331.0)


def grid_side(count: int) -> int:
    """Smallest square grid side that holds `count` chairs."""
    return max(1, math.ceil(math.sqrt(count)))


def chair_positions(count: int) -> list[tuple[float, float, float]]:
    """`count` deterministic chair placements as `(x, z, yaw_degrees)`."""
    if count <= 0:
        return []
    side = grid_side(count)
    centre = (side - 1) / 2.0
    positions: list[tuple[float, float, float]] = []
    for index in range(count):
        column = index % side
        row = index // side
        x = (column - centre) * CHAIR_SPACING
        z = (row - centre) * CHAIR_SPACING
        yaw = YAW_PATTERN[index % len(YAW_PATTERN)]
        positions.append((x, z, yaw))
    return positions


def room_half_extent(count: int) -> float:
    """Half-width of the square room that holds the chair grid."""
    if count <= 0:
        return 8.0
    side = grid_side(count)
    return (side - 1) / 2.0 * CHAIR_SPACING + ROOM_MARGIN


def build_level(level_id: str, name: str, chairs: int) -> dict:
    """A hall with `chairs` chairs, plus the surrounding shell and fixtures."""
    props = [
        {"model": "core:chair", "x": x, "y": 0.0, "z": z, "rotation_degrees": yaw, "scale": 1.0}
        for (x, z, yaw) in chair_positions(chairs)
    ]

    half = room_half_extent(chairs)
    room = {
        "x": -half,
        "z": -half,
        "width": half * 2.0,
        "depth": half * 2.0,
        "height": ROOM_HEIGHT,
    }

    def wall(x: float, z: float, width: float, depth: float) -> dict:
        return {"x": x, "y": 0.0, "z": z, "width": width, "depth": depth, "height": ROOM_HEIGHT}

    t = WALL_THICKNESS
    span = half * 2.0 + t * 2.0
    walls = [
        wall(-half - t, -half - t, span, t),
        wall(-half - t, half, span, t),
        wall(-half - t, -half, t, half * 2.0),
        wall(half, -half, t, half * 2.0),
    ]

    # Fixtures on the centre line keep the hall from being a single unlit
    # rectangle without letting the lighting bake dominate the benchmark.
    fixture_offsets = (-0.5, 0.0, 0.5)
    ceiling_lights = [
        {"fixture": "core:panel_01", "x": 0.0, "z": offset * half}
        for offset in fixture_offsets
    ]

    # Spawn just inside the -Z wall, facing +Z (yaw 180): the whole chair grid
    # is in front of the camera, and yaw 0 puts it all behind.
    spawn_z = -half + 1.0

    return {
        "format_version": 1,
        "id": level_id,
        "name": name,
        "author": "Liminal benchmark generator",
        "spawn": {"x": 0.0, "z": spawn_z, "yaw_degrees": 180.0},
        "defaults": DEFAULTS,
        "room": room,
        "walls": walls,
        "ceiling_lights": ceiling_lights,
        "props": props,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="output directory")
    parser.add_argument(
        "--counts",
        type=int,
        nargs="+",
        default=[0, 25, 50, 75, 100, 150, 200, 300, 400, 500, 750, 1000],
    )
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    for count in args.counts:
        level_id = f"bench_chairs_{count}"
        level = build_level(level_id, f"Bench Chairs {count}", count)
        path = args.out / f"{level_id}.json"
        path.write_text(json.dumps(level, indent=2) + "\n")
        print(f"{path}: {count} chairs")


if __name__ == "__main__":
    main()

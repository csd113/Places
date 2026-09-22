#!/usr/bin/env python3
"""Pool surface artwork: deck tile, wall tile, basin tile and ceiling PNGs.

Goal 5 owns this file.  The Pool set is deliberately clean, pale and
institutional: commercial tile with restrained grout, a sterile ceiling, and a
basin tile that reads at gameplay distance without turning into a grid.

``ART`` maps each logical texture id to its catalog ``model`` path and painter;
``build.py`` merges it into the manifest.  The PNGs are the authoritative
runtime assets -- the game never runs this script, and the artwork can be
replaced by hand.  The shipped Pool sheets are the upgraded 1024x1024
artwork, while this painter is the 128x128 legacy seed: ``build.py`` skips a
shipped sheet whose dimensions differ from its painter's unless ``--force``
is passed.

Painting rules for this set:

* 128x128, 8-bit RGBA, opaque, tileable in both directions (every pattern
  period divides the sheet and every noise helper wraps);
* pale, near-neutral albedo with a faint cool/green cast, because the baked
  lighting multiplies into the sampled texel;
* the grout line is one pixel and barely darker than the tile -- a joint, not
  a debug grid -- with the per-tile tone jitter and the low-frequency field
  carrying the repetition instead of the grid;
* everything deterministic -- only :mod:`artkit` helpers, no randomness, no
  clock, no external images.
"""

from __future__ import annotations

from artkit import (
    Canvas,
    fbm,
    hash01,
    mix_rgb,
    smoothstep,
)

SIZE = 128

# ------------------------------------------------------------------ palette
#
# Albedo before the baked lighting.  The set is off-white / very light
# grey-green with a faint cool cast; the deck is the greenest, the basin the
# coolest (it sits in shadow below the waterline), the wall neutral-pale and
# the ceiling a sterile near-white.

DECK_BASE = (223.0, 226.0, 221.0)     # pale grey-green commercial deck tile
BASIN_BASE = (224.0, 229.0, 228.0)    # a shade cooler and slightly bluer
WALL_BASE = (229.0, 231.0, 226.0)     # the palest of the tile family
CEILING_BASE = (231.0, 232.0, 229.0)  # sterile painted panel, no yellow cast

GROUT_TILE = 0.875   # tile tone multiplied on the 1 px joint (12.5% darker)
GROUT_FLOOR = (206.0, 208.0, 205.0)   # grout is slightly greyer than the tile
GROUT_WALL = (210.0, 212.0, 209.0)

# ------------------------------------------------------------------- helpers


def _tile_indices(x: int, y: int, tiles: int = 10) -> tuple[int, int]:
    """Which tile a texel belongs to (the sheet holds ``tiles`` tiles)."""
    return (x * tiles) // SIZE, (y * tiles) // SIZE


def _on_joint(value: int, tiles: int = 10) -> bool:
    """True when a pixel lies on a tile joint.

    ``(value * tiles) % SIZE < tiles`` marks exactly one pixel per period for
    any tile count that divides the sheet evenly (10 tiles of 12.8 px for the
    tile sheets, 2 panels of 64 px for the ceiling), and the marked pixels are
    the same on both axes, so the joints meet cleanly at every crossing.
    """
    return (value * tiles) % SIZE < tiles


def _tile_variation(tx: int, ty: int, seed: int) -> tuple[float, float]:
    """Per-tile tone multiplier and warm/cool cast, both deterministic."""
    tone = 1.0 + (hash01(tx, ty, seed) - 0.5) * 0.062
    # One tile in seven is a little darker: a delivery batch difference, never
    # a checkerboard.
    if hash01(tx, ty, seed + 3) > 0.858:
        tone *= 0.964
    cast = hash01(tx, ty, seed + 7) - 0.5
    return tone, cast


def _tile_sheet(base, seed: int, joint_rgb, joint_tone: float = GROUT_TILE,
                bevel: float = 0.020, cool: float = 0.0) -> Canvas:
    """One commercial tile sheet: 10x10 tiles, 1 px joints, soft per-tile tone.

    ``cool`` biases the tile (not the joint) a touch towards blue, which keeps
    the basin and the wall distinguishable from the deck without changing the
    family.
    """
    canvas = Canvas(SIZE, SIZE)
    joints_x = [_on_joint(value) for value in range(SIZE)]
    joints_y = [_on_joint(value) for value in range(SIZE)]
    for y in range(SIZE):
        joint_row = joints_y[y]
        for x in range(SIZE):
            on_joint = joints_x[x] or joint_row
            tone, cast = _tile_variation(*_tile_indices(x, y), seed)
            r, g, b = base
            # Gentle 1-2 m field: the era/stain variation across the sheet,
            # far below the per-tile step so no blotch turns into a motif.
            field = fbm(x, y, SIZE, 2, 6, 13, seed + 11)
            tone *= 1.0 + 0.044 * (field - 0.5)
            # Barely visible speckle so a large surface is never dead flat.
            tone *= 1.0 + (hash01(x, y, seed + 17) - 0.5) * 0.020
            if on_joint:
                r, g, b = mix_rgb((r, g, b), joint_rgb, 0.55)
                tone *= joint_tone
            else:
                # A 1 px bevel: the top/left inner edge catches the light and
                # the bottom/right edge settles into it.  It is what makes the
                # joint read as a glaze line at distance.
                if joints_y[y - 1] or joints_x[x - 1]:
                    tone *= 1.0 + bevel
                if joints_y[(y + 1) % SIZE] or joints_x[(x + 1) % SIZE]:
                    tone *= 1.0 - bevel
            r = r * tone * (1.0 + 0.012 * cast - 0.010 * cool)
            g = g * tone
            b = b * tone * (1.0 - 0.010 * cast + 0.026 * cool)
            canvas.set(x, y, (r, g, b))
    return canvas


# --------------------------------------------------------------- tile sheets


def build_deck() -> Canvas:
    """Pool deck tile: 15 cm tiles at a 1.5 m repeat (12.8 px per tile)."""
    return _tile_sheet(DECK_BASE, 121, GROUT_FLOOR, cool=0.0)


def build_basin() -> Canvas:
    """Basin tile: 10 cm tiles at a 1 m repeat, cooler below the waterline."""
    return _tile_sheet(BASIN_BASE, 137, GROUT_FLOOR, cool=0.55)


def build_wall() -> Canvas:
    """Wall tile: 10 cm tiles at a 1 m repeat, the palest of the family."""
    return _tile_sheet(WALL_BASE, 149, GROUT_WALL, bevel=0.024, cool=0.35)


# ------------------------------------------------------------------ ceiling


def build_ceiling() -> Canvas:
    """Sterile painted ceiling: 2x2 panels, 1 m each, at a 2 m repeat.

    A commercial painted panel, not the office's yellowed acoustic tile: fine
    joints, a soft edge roll, roller striation, a faint field and four screw
    dimples per panel.  No T-bar, no grid motif, no water damage.
    """
    canvas = Canvas(SIZE, SIZE)
    half = SIZE // 2
    panel_tone = (
        1.0 + (hash01(0, 0, 191) - 0.5) * 0.030,
        1.0 + (hash01(1, 0, 191) - 0.5) * 0.030,
        1.0 + (hash01(0, 1, 191) - 0.5) * 0.030,
        1.0 + (hash01(1, 1, 191) - 0.5) * 0.030,
    )
    for y in range(SIZE):
        for x in range(SIZE):
            px, py = x // half, y // half
            panel = px + 2 * py
            seam = (x % half == 0) or (y % half == 0)
            # Distance to the panel edge, 0 at the joint.
            edge = min(x % half, half - 1 - (x % half), y % half, half - 1 - (y % half))
            tone = panel_tone[panel]
            # A gentle roll down to the joint: 3 px of soft shadow, then flat.
            tone *= 1.0 - 0.042 * (1.0 - smoothstep(float(edge), 0.0, 4.0))
            # Fine roller drag, mostly horizontal, plus a faint broad field.
            tone *= 1.0 + 0.006 * (hash01(x >> 2, y, 197) - 0.5)
            field = fbm(x, y, SIZE, 3, 7, 17, 199)
            tone *= 1.0 + 0.030 * (field - 0.5)
            tone *= 1.0 + (hash01(x, y, 211) - 0.5) * 0.014
            if seam:
                tone *= 0.900
            elif x % half == 1 or y % half == 1:
                # The lit lip of the joint.
                tone *= 1.016
            # Four countersunk screw dimples per panel, well inside the edge.
            local_x = x % half
            local_y = y % half
            if (local_x in (9, 54)) and (local_y in (9, 54)):
                tone *= 0.930
            canvas.set(
                x,
                y,
                (
                    CEILING_BASE[0] * tone,
                    CEILING_BASE[1] * tone,
                    CEILING_BASE[2] * tone * 1.004,
                ),
            )
    return canvas


# ------------------------------------------------------------------ manifest

ART = {
    "core:tex_pool_tile_deck_01": {
        "model": "environment/pool/textures/floors/pool_tile_deck_01.png",
        "build": build_deck,
    },
    "core:tex_pool_tile_basin_01": {
        "model": "environment/pool/textures/floors/pool_tile_basin_01.png",
        "build": build_basin,
    },
    "core:tex_pool_tile_wall_01": {
        "model": "environment/pool/textures/walls/pool_tile_wall_01.png",
        "build": build_wall,
    },
    "core:tex_pool_ceiling_01": {
        "model": "environment/pool/textures/ceilings/pool_ceiling_01.png",
        "build": build_ceiling,
    },
}

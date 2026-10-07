#!/usr/bin/env python3
"""Office surface artwork, loaded from the authoritative committed PNGs.

All six manifest builders retain the authored 1024-square sheets, including
forced builds. Historical low-resolution paint helpers remain below for
reference; they are not the production export path. No artwork is generated
at runtime. Prop master-to-native conversion lives in
``tools/props/build_office_textures.py``.
"""

from __future__ import annotations

from artkit import (
    Canvas,
    clamp,
    fbm,
    hash01,
    smoothstep,
    tile_noise,
)

# Historical helpers use 128px, with 64px per metre at tile_metres = 2.0.
# Production manifest exporters below load the committed 1024px PNGs.
SIZE = 128
HALF = SIZE // 2

# ------------------------------------------------------------------ palette
#
# Base colours are the albedo *before* the material tint and the baked
# lighting, so they stay pale and only gently warm/cool.

PAPER_BASE = (242.0, 237.0, 222.0)      # cream printing stock
PAPER_AGE = (0.035, 0.030, 0.020)       # warm-grey patina strength

PILE_BASE = (124.0, 109.0, 78.0)       # muted warm-brown short-pile carpet

TILE_BASE = (236.0, 232.0, 221.0)       # slightly yellowed acoustic tile
BAR_BASE = (199.0, 197.0, 190.0)        # painted T-bar steel
# Per-panel tone offsets: four nominally identical tiles never are.
PANEL_TONES = (1.000, 0.986, 1.010, 0.976)

STAIN_PAPER = (146.0, 120.0, 84.0)      # rusty grey-brown soaked wallpaper
STAIN_PILE = (104.0, 95.0, 78.0)        # damp, flattened carpet
STAIN_TILE = (134.0, 107.0, 74.0)       # ceiling water mark
TIDE_TILE = (92.0, 68.0, 46.0)          # the darker tide ring


def _wrap(value: int, period: int) -> int:
    """Positive modulo, so wrapped hash lookups stay inside the sheet."""
    return value % period


def _wet_mask(x: int, y: int, coarse: int, fine: int, finer: int, seed: int) -> float:
    """A soft damp field in 0..1.

    Water damage reads as a gradual darkening, so the field is mapped over a
    wide range and never thresholded into shapes: a hard outline on a 2 m
    sheet is the thing that turns damage into a repeating pattern.
    """
    field = fbm(x, y, SIZE, coarse, fine, finer, seed)
    return clamp((field - 0.35) / 0.45, 0.0, 1.0)


# ---------------------------------------------------------------- wallpaper


def _paper_stock(x: int, y: int) -> float:
    """Fine vertical striation of the printed stock, 1-3 px noise."""
    striation = (hash01(x, y, 11) - 0.5) * 0.020
    column = (hash01(x, y >> 2, 13) - 0.5) * 0.024
    return 1.0 + striation + column


def _paper_print(x: int, y: int) -> float:
    """The commercial print: pinstripe pair plus a half-drop dot lattice.

    Cells are 16 px (25 cm); the dot grid steps half a cell per row, which is
    how a modest office wallcovering repeats.  Contrast stays in the low
    single digits so the pattern is read at arm's length, not across the room.
    """
    px = x % 16
    py = y % 16
    tone = 1.0
    if px == 0:
        tone *= 0.968                    # the broad stripe joint
    elif px == 2:
        tone *= 0.985                    # companion line
    elif px == 9:
        tone *= 1.010                    # printed highlight
    if (px == 5 and py == 8) or (px == 13 and py == 0):
        tone *= 0.958                    # the dot motif
    return tone


def wallpaper_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the pale printed wallpaper (2 m, 16 px stripe cells)."""
    tone = _paper_stock(x, y) * _paper_print(x, y)
    age = fbm(x, y, SIZE, 4, 11, 23, 29) - 0.5
    tone *= 1.0 + PAPER_AGE[0] * age
    # A sparse soft vertical smudge per repeat: past cleaning, not water.
    smudge = clamp((tile_noise(x, 0, SIZE, 16, 31) - 0.70) / 0.30, 0.0, 1.0)
    tone *= 1.0 - 0.028 * smudge
    warm = 0.5 * age
    return (
        PAPER_BASE[0] * tone * (1.0 + 0.018 * warm),
        PAPER_BASE[1] * tone,
        PAPER_BASE[2] * tone * (1.0 - 0.030 * warm),
    )


def load_office_sheet(folder: str, name: str, dimensions=(1024, 1024)) -> Canvas:
    """Load committed Office artwork; forced builds preserve its authored layout."""
    from pathlib import Path
    from seam_repair import read_png

    source = Path(__file__).resolve().parents[2] / "assets/environment/office/textures" / folder / name
    image = read_png(str(source))
    if (image.width, image.height) != dimensions:
        raise ValueError(f"{name} must retain its {dimensions} contract")
    canvas = Canvas(image.width, image.height)
    for i in range(image.width * image.height):
        start = i * image.channels
        canvas.pixels[i*4:i*4+3] = image.pixels[start:start+3]
        if image.channels == 4:
            canvas.pixels[i*4+3] = image.pixels[start+3]
    return canvas


def build_wallpaper_yellow() -> Canvas:
    return load_office_sheet("walls", "wallpaper_yellow_01.png")


def build_wallpaper_stained() -> Canvas:
    return load_office_sheet("walls", "wallpaper_stained_01.png")


# ------------------------------------------------------------------- carpet


def _pile_fibre(x: int, y: int, seed: int) -> float:
    """Short horizontal smear of the per-texel hash: the pile direction.

    Sampling the wrapped neighbours keeps the smear seamless across the sheet
    edge, and smearing along x only makes the noise read as directional fibre
    instead of isotropic static.
    """
    value = 0.50 * hash01(x, y, seed)
    value += 0.30 * hash01(_wrap(x - 1, SIZE), y, seed)
    value += 0.20 * hash01(_wrap(x + 1, SIZE), y, seed)
    return value


def carpet_rgb(x: int, y: int, damp: bool) -> tuple[float, float, float]:
    """One texel of the short-pile carpet (no metre checker anywhere).

    The read comes from fine directional fibre and pile loops; the broad
    mottle stays gentle, because a strong metre-scale blotch on a 2 m sheet
    repeats visibly and starts to look like a pattern.
    """
    mottle = fbm(x, y, SIZE, 3, 7, 17, 41) - 0.5      # gentle 0.5-2 m wear
    patch = tile_noise(x, y, SIZE, 9, 43) - 0.5       # 22 cm patches
    fibre = _pile_fibre(x, y, 47) - 0.5               # pile direction
    loops = hash01(x >> 1, y, 59) - 0.5               # 2 px pile loops
    tuft = hash01(x, y, 53) - 0.5                     # per-loop flicker
    wet = 0.0
    if damp:
        wet = _wet_mask(x, y, 2, 4, 9, 201)
    # Damp pile flattens: the soaked core loses its fibre contrast.
    fibre_amplitude = 0.115 * (1.0 - 0.40 * wet)
    tone = (
        1.0
        + 0.055 * mottle
        + 0.050 * patch
        + fibre_amplitude * fibre
        + 0.052 * loops
        + 0.030 * tuft
    )
    r = PILE_BASE[0] * tone * (1.0 + 0.022 * mottle)
    g = PILE_BASE[1] * tone
    b = PILE_BASE[2] * tone * (1.0 - 0.032 * mottle)
    if damp:
        # Damp pile darkens and cools slightly, in a soft field only: small
        # amplitudes keep it from reading as a repeating camouflage pattern.
        dampen = 1.0 - 0.26 * wet
        r = r * dampen * (1.0 - 0.015 * wet)
        g = g * dampen
        b = b * dampen * (1.0 + 0.025 * wet)
        mix = 0.30 * wet
        r = STAIN_PILE[0] * mix + r * (1.0 - mix)
        g = STAIN_PILE[1] * mix + g * (1.0 - mix)
        b = STAIN_PILE[2] * mix + b * (1.0 - mix)
    return r, g, b


def carpet_canvas(damp: bool) -> Canvas:
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, carpet_rgb(x, y, damp))
    return canvas


def build_carpet_beige() -> Canvas:
    return load_office_sheet("floors", "carpet_beige_01.png")


def build_carpet_damp() -> Canvas:
    return load_office_sheet("floors", "carpet_damp_01.png")


# ------------------------------------------------------------------ ceiling


def _ceiling_geometry(x: int, y: int):
    """Panel-local coordinates for the 2x2 suspended ceiling."""
    tx = x % HALF
    ty = y % HALF
    tile = (x // HALF) + 2 * (y // HALF)
    edge = min(tx, HALF - 1 - tx, ty, HALF - 1 - ty)
    return tx, ty, tile, edge


def ceiling_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the aged 2 m suspended ceiling: four 1 m acoustic panels
    under a thin T-bar grid.  The grid is 2 px of bar plus a 1 px shadow
    groove per side, which reads at a glance without dominating the tile."""
    tx, ty, tile, edge = _ceiling_geometry(x, y)
    # Acoustic tile: pinholes, fine grain and blotchy grime shared across the
    # panel joints (real grime does not stop at the T-bar).
    pore = -0.055 if hash01(x, y, 71) > 0.918 else 0.0
    grain = (hash01(x, y, 67) - 0.5) * 0.032
    blotch = tile_noise(x, y, SIZE, 9, 73) - 0.5
    tone = PANEL_TONES[tile] * (1.0 + grain + pore + 0.030 * blotch)
    # A little yellowing that varies down the length of the sheet.
    yellow = clamp((tile_noise(x, 0, SIZE, 5, 77) - 0.35) / 0.5, 0.0, 1.0)
    bar = 1.0 + 0.035 * (tile_noise(x, y, SIZE, 32, 79) - 0.5)
    if edge <= 1:
        base, factor = BAR_BASE, bar
    elif edge == 2:
        base, factor = TILE_BASE, tone * 0.905
    else:
        base, factor = TILE_BASE, tone
    return (
        base[0] * factor * (1.0 + 0.015 * yellow),
        base[1] * factor * (1.0 + 0.010 * yellow),
        base[2] * factor * (1.0 - 0.030 * yellow),
    )


def build_ceiling_panel() -> Canvas:
    return load_office_sheet("ceilings", "ceiling_panel_01.png")


def build_ceiling_stained() -> Canvas:
    return load_office_sheet("ceilings", "ceiling_stained_01.png")


# ------------------------------------------------------------------ manifest

ART = {
    "core:tex_wallpaper_yellow_01": {
        "model": "environment/office/textures/walls/wallpaper_yellow_01.png",
        "build": build_wallpaper_yellow,
    },
    "core:tex_wallpaper_stained_01": {
        "model": "environment/office/textures/walls/wallpaper_stained_01.png",
        "build": build_wallpaper_stained,
    },
    "core:tex_carpet_beige_01": {
        "model": "environment/office/textures/floors/carpet_beige_01.png",
        "build": build_carpet_beige,
    },
    "core:tex_carpet_damp_01": {
        "model": "environment/office/textures/floors/carpet_damp_01.png",
        "build": build_carpet_damp,
    },
    "core:tex_ceiling_panel_01": {
        "model": "environment/office/textures/ceilings/ceiling_panel_01.png",
        "build": build_ceiling_panel,
    },
    "core:tex_ceiling_stained_01": {
        "model": "environment/office/textures/ceilings/ceiling_stained_01.png",
        "build": build_ceiling_stained,
    },
}

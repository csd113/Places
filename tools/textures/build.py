#!/usr/bin/env python3
"""Authors and validates the environment surface PNGs.

These are deliberately temporary **seed** assets: Goal 5 replaces them with real
artwork.  Until then the tool bakes the renderer's procedural office materials
(originally in ``src/render.rs``: wallpaper, carpet, panel ceiling and their
water-damaged variants) into tileable PNG files that ``assets/catalog.json``
registers as ``asset_type: "texture"`` entries.

The painter is pure stdlib -- no PIL, no numpy -- and deterministic: running it
twice produces byte-identical files, so the shipped PNGs can be regenerated and
diffed like source.  It also owns the diagnostic textures that prove arbitrary
PNG dimensions and alpha decode on the real renderer.

Run it from the repository root::

    python3 tools/textures/build.py           # (re)generate every manifest texture
    python3 tools/textures/build.py --check   # validate the shipped PNGs only

``--check`` never regenerates: it reads the catalog, parses each texture PNG's
IHDR and fails on missing/corrupt/oversized files (hard limit 1024x1024,
preferred 256x256, power-of-two dimensions preferred).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import struct
import sys
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
PACKAGE_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ASSET_ROOT = os.path.join(PACKAGE_ROOT, "assets")
CATALOG_PATH = os.path.join(ASSET_ROOT, "catalog.json")

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
# Budgets for the ES 2.0 / Mali-400 target.  256x256 is preferred; 1024x1024
# is the hard ceiling (see the README and assets/README.md).
PREFERRED_DIMENSION = 256
HARD_DIMENSION = 1024

_MASK32 = 0xFFFFFFFF


# --------------------------------------------------------------- PNG writer
#
# Copied from tools/props/tex.py on purpose: the texture tool must stay
# runnable on its own (the prop toolkit imports from its own directory).


def write_png(width: int, height: int, rgba: bytes) -> bytes:
    """Encodes 8-bit RGBA pixels as a PNG (filter 0, no interlacing)."""
    if len(rgba) != width * height * 4:
        raise ValueError("rgba buffer length does not match dimensions")
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)  # filter type: None
        raw += rgba[y * stride : (y + 1) * stride]

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + tag
            + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (
        PNG_SIGNATURE
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


# ------------------------------------------------------------ texture noise
#
# A small port of src/render.rs.  Approximate, not bit-parity: the point is the
# same look (wrapped hash noise, no seams) with the same deterministic output
# on every run.


def hash01(x: int, y: int, seed: int) -> float:
    """Deterministic per-texel hash in [0, 1), matching the renderer's mix."""
    h = ((x & _MASK32) * 0x9E3779B9) & _MASK32
    h ^= ((y & _MASK32) * 0x85EBCA6B) & _MASK32
    h ^= ((seed & _MASK32) * 0xC2B2AE35) & _MASK32
    h &= _MASK32
    h ^= h >> 15
    h = (h * 0x2545F491) & _MASK32
    h ^= h >> 13
    h = (h * 0x27D4EB2D) & _MASK32
    h ^= h >> 16
    return (h & 0x00FFFFFF) / 16777216.0


def tile_noise(x: int, y: int, size: int, period: int, seed: int) -> float:
    """Tileable value noise in [0, 1] over a ``size`` square, wrapped at ``period``."""
    period = max(1, period)
    scale = period / float(size)
    fx = x * scale
    fy = y * scale
    x0 = math.floor(fx)
    y0 = math.floor(fy)
    tx = fx - x0
    ty = fy - y0
    sx = tx * tx * (3.0 - 2.0 * tx)
    sy = ty * ty * (3.0 - 2.0 * ty)
    ix0 = int(x0) % period
    iy0 = int(y0) % period
    ix1 = (int(x0) + 1) % period
    iy1 = (int(y0) + 1) % period
    v00 = hash01(ix0, iy0, seed)
    v10 = hash01(ix1, iy0, seed)
    v01 = hash01(ix0, iy1, seed)
    v11 = hash01(ix1, iy1, seed)
    top = v00 + (v10 - v00) * sx
    bottom = v01 + (v11 - v01) * sx
    return top + (bottom - top) * sy


def tile_noise2(x: int, y: int, size: int, coarse: int, fine: int, seed: int) -> float:
    """Two octaves of tileable noise, the shape most of the surface ageing uses."""
    value = 0.65 * tile_noise(x, y, size, coarse, seed) + 0.35 * tile_noise(
        x, y, size, fine, seed + 7
    )
    return max(0.0, min(1.0, value))


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


# ------------------------------------------------------------------- canvas


class Canvas:
    """An 8-bit RGBA buffer with the handful of painting operations we need."""

    def __init__(self, width: int, height: int, fill: tuple[int, int, int, int] = (255, 255, 255, 255)) -> None:
        self.width = width
        self.height = height
        self.pixels = bytearray(bytes(fill) * (width * height))

    def set(self, x: int, y: int, rgb: tuple[float, float, float], alpha: int = 255) -> None:
        if x < 0 or y < 0 or x >= self.width or y >= self.height:
            return
        index = (y * self.width + x) * 4
        self.pixels[index] = int(round(clamp(rgb[0], 0.0, 255.0)))
        self.pixels[index + 1] = int(round(clamp(rgb[1], 0.0, 255.0)))
        self.pixels[index + 2] = int(round(clamp(rgb[2], 0.0, 255.0)))
        self.pixels[index + 3] = int(round(clamp(float(alpha), 0.0, 255.0)))

    def rect(self, x0: int, y0: int, x1: int, y1: int, rgb, alpha: int = 255) -> None:
        """Filled inclusive rectangle, clipped to the canvas."""
        for y in range(max(0, y0), min(self.height - 1, y1) + 1):
            for x in range(max(0, x0), min(self.width - 1, x1) + 1):
                self.set(x, y, rgb, alpha)

    def disc(self, cx: int, cy: int, radius: int, rgb, alpha: int = 255) -> None:
        """Filled circle; alpha 0 acts as an eraser when repainting RGBA."""
        limit = radius * radius + 0.5
        for y in range(cy - radius, cy + radius + 1):
            for x in range(cx - radius, cx + radius + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= limit:
                    self.set(x, y, rgb, alpha)

    def triangle_up(self, cx: int, apex_y: int, base_y: int, half_width: int, rgb, alpha: int = 255) -> None:
        """Up-pointing triangle: apex on top, widening towards ``base_y``."""
        span = max(1, base_y - apex_y)
        for y in range(apex_y, base_y + 1):
            half = int(round((y - apex_y) / span * half_width))
            for x in range(cx - half, cx + half + 1):
                self.set(x, y, rgb, alpha)

    def triangle_right(self, tip_x: int, base_x: int, cy: int, half_height: int, rgb, alpha: int = 255) -> None:
        """Right-pointing triangle: base on the left, tip at ``tip_x``."""
        span = max(1, tip_x - base_x)
        for x in range(base_x, tip_x + 1):
            half = int(round((x - base_x) / span * half_height))
            for y in range(cy - half, cy + half + 1):
                self.set(x, y, rgb, alpha)

    def rgba(self) -> bytes:
        return bytes(self.pixels)


# ------------------------------------------------------- office surface art


PAPER = (243.0, 237.0, 220.0)
PILE = (231.0, 223.0, 210.0)
CEILING_TILE = (247.0, 247.0, 242.0)
CEILING_BAR = (168.0, 168.0, 162.0)
CEILING_DIP = (0.52, 0.66, 0.84, 0.95)
CEILING_STAIN = (128.0, 98.0, 62.0)

# The historical 1 m checker tint from src/render.rs, close together on
# purpose: the metre checker reads as uneven carpet wear, not as a tiled floor.
CHECKER_BRIGHT = (0.550, 0.500, 0.383)
CHECKER_DARK = (0.518, 0.471, 0.360)


def wallpaper_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the pale two-tone printed wallpaper (2 m, 16 px stripes)."""
    phase = x % 16
    tone = 0.997 if phase < 7 else 0.928
    if phase in (7, 15):
        tone *= 0.960
    elif phase == 3:
        tone *= 1.014
    fibre = (hash01(x, y, 11) - 0.5) * 0.030
    age = tile_noise2(x, y, 128, 6, 17, 23) - 0.5
    tone *= 1.0 + fibre + 0.075 * age
    return (
        PAPER[0] * tone,
        PAPER[1] * tone * (1.0 - 0.008 * age),
        PAPER[2] * tone * (1.0 - 0.022 * age),
    )


def build_wallpaper_yellow() -> Canvas:
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            canvas.set(x, y, wallpaper_rgb(x, y))
    return canvas


def build_wallpaper_stained() -> Canvas:
    """The same printed paper with restrained water damage."""
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            r, g, b = wallpaper_rgb(x, y)
            # Broad damp fields at a 25-60 cm scale.
            field = tile_noise2(x, y, 128, 3, 8, 101)
            wet = clamp((field - 0.62) / 0.24, 0.0, 1.0)
            shoulder = clamp((field - 0.52) / 0.24, 0.0, 1.0)
            # Vertical runs: a column mask times a length mask, so a run stays
            # continuous down the whole two metre repeat.
            column = tile_noise(x, 0, 128, 9, 103)
            feather = tile_noise(x, 0, 128, 21, 105)
            length = tile_noise(0, y, 128, 5, 107)
            run = clamp((0.35 * feather + column - 0.62) / 0.20, 0.0, 1.0) * clamp(
                (length - 0.28) / 0.44, 0.0, 1.0
            )
            fibre = (hash01(x, y, 109) - 0.5) * 0.05
            darken = 1.0 - 0.06 * shoulder - 0.05 * wet - 0.11 * run - 0.03 * fibre
            # Soaked paper loses its yellow and picks up a rusty grey-brown.
            warmth = 0.12 * run + 0.14 * shoulder
            canvas.set(
                x,
                y,
                (
                    r * darken * (1.0 + warmth * 0.30),
                    g * darken * (1.0 + warmth * 0.02),
                    b * darken * (1.0 - warmth * 0.55),
                ),
            )
    return canvas


def carpet_rgb(x: int, y: int, damp: bool) -> tuple[float, float, float]:
    """One texel of the 1 m short-pile carpet source (x and y in 0..63)."""
    speckle = hash01(x, y, 31) - 0.5
    dash_v = hash01(x, y >> 1, 37) - 0.5
    dash_h = hash01(x >> 1, y, 41) - 0.5
    tuft = tile_noise(x, y, 64, 21, 45) - 0.5
    mottle = tile_noise(x, y, 64, 5, 43) - 0.5
    broad = tile_noise(x, y, 64, 13, 47) - 0.5
    warm = tile_noise(x, y, 64, 3, 53) - 0.5
    tone = (
        1.0
        + 0.070 * speckle
        + 0.050 * dash_v
        + 0.035 * dash_h
        + 0.035 * tuft
        + 0.060 * mottle
        + 0.040 * broad
    )
    r = PILE[0] * tone * (1.0 + 0.020 * warm)
    g = PILE[1] * tone
    b = PILE[2] * tone * (1.0 - 0.028 * warm)
    if damp:
        field = tile_noise2(x, y, 64, 3, 6, 201)
        wet = clamp((field - 0.52) / 0.24, 0.0, 1.0)
        margin = clamp((field - 0.40) / 0.24, 0.0, 1.0) - wet
        flatten = clamp((tile_noise(x, y, 64, 11, 203) - 0.60) / 0.30, 0.0, 1.0) * wet
        darken = 1.0 - 0.30 * wet - 0.08 * flatten - 0.07 * margin
        warmth = 0.55 * wet
        r = r * darken * (1.0 + 0.03 * warmth)
        g = g * darken
        b = b * darken * (1.0 - 0.06 * warmth)
    return r, g, b


def carpet_canvas(damp: bool) -> Canvas:
    """A 2 m carpet sheet: four 1 m quadrants with the checker tint baked in.

    Quadrant (0, 0) -- the top-left 64x64 square -- is the bright cell, exactly
    the phase ``generate_floor_checker_texture`` uses in ``src/render.rs``.
    """
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            r, g, b = carpet_rgb(x % 64, y % 64, damp)
            bright = ((x // 64) + (y // 64)) % 2 == 0
            tint = CHECKER_BRIGHT if bright else CHECKER_DARK
            canvas.set(x, y, (r * tint[0], g * tint[1], b * tint[2]))
    return canvas


def build_carpet_beige() -> Canvas:
    return carpet_canvas(damp=False)


def build_carpet_damp() -> Canvas:
    return carpet_canvas(damp=True)


def ceiling_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the 2 m suspended ceiling: four 1 m tiles, 3 cm T-bar grid."""
    tx = x % 64
    ty = y % 64
    edge = min(tx, 63 - tx, ty, 63 - ty)
    tile = (x // 64) + 2 * (y // 64)
    tile_tone = 1.0 + 0.024 * (hash01(tile, tile * 7, 61) - 0.5)
    fibre = (hash01(x, y, 67) - 0.5) * 0.045
    pores = -0.075 if hash01(x, y, 71) > 0.945 else 0.0
    blotch = tile_noise(x, y, 128, 9, 73) - 0.5
    field = tile_tone * (1.0 + fibre + pores + 0.035 * blotch)
    dip = CEILING_DIP[edge] if edge < 4 else 1.0
    bar_mix = (1.0 - 0.35 * edge) if edge < 2 else 0.0
    return (
        CEILING_BAR[0] * bar_mix * field + CEILING_TILE[0] * field * dip * (1.0 - bar_mix),
        CEILING_BAR[1] * bar_mix * field + CEILING_TILE[1] * field * dip * (1.0 - bar_mix),
        CEILING_BAR[2] * bar_mix * field + CEILING_TILE[2] * field * dip * (1.0 - bar_mix),
    )


def build_ceiling_panel() -> Canvas:
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            canvas.set(x, y, ceiling_rgb(x, y))
    return canvas


def build_ceiling_stained() -> Canvas:
    """One panel carries a brown tide stain, the neighbours stay lightly aged."""
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            r, g, b = ceiling_rgb(x, y)
            tx = x % 64
            ty = y % 64
            tile = (x // 64) + 2 * (y // 64)
            severity = 1.0 if tile == 3 else (0.38 if tile == 1 else 0.14)
            dx = float(min(tx, 63 - tx))
            dy = float(min(ty, 63 - ty))
            edge = min(dx, dy)
            field = tile_noise2(x, y, 128, 3, 9, 301)
            spread = clamp((field - 0.48) / 0.32, 0.0, 1.0)
            shoulder = clamp((field - 0.36) / 0.32, 0.0, 1.0)
            trail = 0.0
            if tile in (2, 3):
                trail = (
                    (1.0 - clamp(dx / 40.0, 0.0, 1.0))
                    * 0.34
                    * clamp((tile_noise(x, y, 128, 5, 307) - 0.46) / 0.30, 0.0, 1.0)
                )
                if tile == 2:
                    trail *= 0.45
            # Water collects against the grid and the metal T-bar interrupts it.
            grid_fade = clamp(edge / 6.0, 0.0, 1.0)
            stain = clamp((0.62 * spread + 0.30 * shoulder + trail) * severity, 0.0, 1.0) * grid_fade
            mix = 0.40 * stain
            canvas.set(
                x,
                y,
                (
                    CEILING_STAIN[0] * mix + r * (1.0 - mix),
                    CEILING_STAIN[1] * mix + g * (1.0 - mix),
                    CEILING_STAIN[2] * mix + b * (1.0 - mix),
                ),
            )
    return canvas


# ------------------------------------------------------- diagnostic surface


WHITE = (245, 245, 245)
BLACK = (28, 28, 34)
RED = (220, 45, 45)
GREEN = (35, 165, 70)
BLUE = (45, 85, 215)
YELLOW = (245, 205, 45)
NAVY = (32, 58, 140)
MAGENTA = (225, 45, 200)
ORANGE = (235, 130, 35)
CYAN = (35, 195, 205)


def corner_markers(canvas: Canvas) -> None:
    """Red/green/blue/yellow corner blocks with deliberately different sizes."""
    for x0, y0, x1, y1, color in (
        (0, 0, 23, 23, RED),
        (107, 0, 127, 19, GREEN),
        (0, 109, 15, 127, BLUE),
        (117, 117, 127, 127, YELLOW),
    ):
        canvas.rect(x0 - 2, y0 - 2, x1 + 2, y1 + 2, WHITE)
        canvas.rect(x0, y0, x1, y1, color)


def build_diagnostic_wall() -> Canvas:
    """Blue/white diagonal stripes, a bold up arrow, asymmetric corner blocks."""
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            canvas.set(x, y, NAVY if ((x + y) // 16) % 2 == 0 else WHITE)
    corner_markers(canvas)
    canvas.triangle_up(64, 13, 51, 33, BLACK)
    canvas.rect(46, 40, 82, 111, BLACK)
    canvas.triangle_up(64, 18, 48, 30, WHITE)
    canvas.rect(49, 45, 79, 108, WHITE)
    return canvas


def build_diagnostic_floor() -> Canvas:
    """Magenta/white 1 m checker with a large right-pointing arrow."""
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            cell = (x // 64) + (y // 64)
            canvas.set(x, y, MAGENTA if cell % 2 == 0 else WHITE)
    canvas.rect(12, 50, 82, 78, WHITE)
    canvas.triangle_right(116, 68, 64, 34, WHITE)
    canvas.rect(16, 54, 78, 74, BLACK)
    canvas.triangle_right(112, 72, 64, 30, BLACK)
    corner_markers(canvas)
    return canvas


def build_diagnostic_ceiling() -> Canvas:
    """Green/white concentric square rings plus the corner markers."""
    canvas = Canvas(128, 128)
    for y in range(128):
        for x in range(128):
            edge = min(x, y, 127 - x, 127 - y)
            canvas.set(x, y, GREEN if (edge // 16) % 2 == 0 else WHITE)
    corner_markers(canvas)
    return canvas


def build_diagnostic_alt() -> Canvas:
    """96x64 orange/cyan checker with a striped border (non-power-of-two)."""
    canvas = Canvas(96, 64)
    for y in range(64):
        for x in range(96):
            border = x < 8 or y < 8 or x >= 88 or y >= 56
            if border:
                canvas.set(x, y, CYAN if ((x + y) // 8) % 2 == 0 else ORANGE)
            else:
                cell = ((x - 8) // 16) + ((y - 8) // 16)
                canvas.set(x, y, ORANGE if cell % 2 == 0 else CYAN)
    # Two asymmetric patches so one corner of the repeat is unmistakable.
    canvas.rect(12, 12, 23, 23, WHITE)
    canvas.rect(76, 44, 83, 51, BLACK)
    return canvas


def build_diagnostic_alpha() -> Canvas:
    """Opaque centre disc/arrow, a half-transparent ring, a clear outer margin."""
    canvas = Canvas(128, 128, fill=(0, 0, 0, 0))
    canvas.disc(64, 64, 52, (255, 214, 64), alpha=140)
    canvas.disc(64, 64, 39, (0, 0, 0), alpha=0)
    canvas.disc(64, 64, 32, (38, 88, 200))
    canvas.rect(58, 46, 69, 84, WHITE)
    canvas.triangle_up(64, 36, 55, 20, WHITE)
    return canvas


# ---------------------------------------------------------------- manifest
#
# id -> (catalog model path relative to assets/, painter).  Mirrors the
# asset_type "texture" entries in assets/catalog.json; --check warns on drift.

MANIFEST = {
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
    "core:tex_diagnostic_wall_01": {
        "model": "diagnostic/textures/diagnostic_wall_01.png",
        "build": build_diagnostic_wall,
    },
    "core:tex_diagnostic_floor_01": {
        "model": "diagnostic/textures/diagnostic_floor_01.png",
        "build": build_diagnostic_floor,
    },
    "core:tex_diagnostic_ceiling_01": {
        "model": "diagnostic/textures/diagnostic_ceiling_01.png",
        "build": build_diagnostic_ceiling,
    },
    "core:tex_diagnostic_alt_01": {
        "model": "diagnostic/textures/diagnostic_alt_01.png",
        "build": build_diagnostic_alt,
    },
    "core:tex_diagnostic_alpha_01": {
        "model": "diagnostic/textures/diagnostic_alpha_01.png",
        "build": build_diagnostic_alpha,
    },
}


# ---------------------------------------------------------------- validation


def is_power_of_two(value: int) -> bool:
    return value > 0 and (value & (value - 1)) == 0


def read_png_dimensions(path: str) -> tuple[int, int]:
    """Parses a PNG's signature and IHDR, returning ``(width, height)``."""
    with open(path, "rb") as handle:
        data = handle.read()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("missing PNG signature")
    if len(data) < 33:
        raise ValueError("truncated PNG")
    length = int.from_bytes(data[8:12], "big")
    tag = data[12:16]
    if tag != b"IHDR" or length != 13:
        raise ValueError("the first chunk is not a 13-byte IHDR")
    if data[-8:-4] != b"IEND":
        raise ValueError("missing IEND chunk")
    return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")


def validate_textures(
    catalog_path: str = CATALOG_PATH, asset_root: str = ASSET_ROOT
) -> tuple[list[str], list[str], list[str]]:
    """Returns ``(errors, warnings, report)`` for the catalog's texture assets."""
    errors: list[str] = []
    warnings: list[str] = []
    report: list[str] = []
    try:
        with open(catalog_path, "r", encoding="utf-8") as handle:
            catalog = json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        return [f"catalog: {error}"], warnings, report

    catalog_ids: list[str] = []
    for entry in catalog.get("assets", []):
        if entry.get("asset_type") != "texture":
            continue
        texture_id = str(entry.get("id", "")).strip() or "texture"
        catalog_ids.append(str(entry.get("id", "")).strip())
        model = entry.get("model")
        if not isinstance(model, str) or not model.strip():
            errors.append(f"{texture_id}: texture asset has no model path")
            continue
        model = model.strip()
        path = os.path.join(asset_root, model)
        if not os.path.isfile(path):
            errors.append(f"{texture_id}: file '{model}' is missing below assets/")
            continue
        try:
            width, height = read_png_dimensions(path)
        except (OSError, ValueError) as error:
            errors.append(f"{texture_id}: '{model}' is not a valid PNG ({error})")
            continue
        if width <= 0 or height <= 0:
            errors.append(f"{texture_id}: '{model}' has zero pixels ({width}x{height})")
            continue
        if width > HARD_DIMENSION or height > HARD_DIMENSION:
            errors.append(
                f"{texture_id}: '{model}' is {width}x{height}, over the {HARD_DIMENSION}x{HARD_DIMENSION} hard limit"
            )
        if width > PREFERRED_DIMENSION or height > PREFERRED_DIMENSION:
            warnings.append(
                f"{texture_id}: '{model}' is {width}x{height}, over the preferred {PREFERRED_DIMENSION}x{PREFERRED_DIMENSION}"
            )
        if not is_power_of_two(width) or not is_power_of_two(height):
            warnings.append(f"{texture_id}: '{model}' is {width}x{height}, not a power of two")
        report.append(f"OK {texture_id}: {model} {width}x{height}")

    catalog_set = set(catalog_ids)
    manifest_set = set(MANIFEST)
    for texture_id in sorted(catalog_set - manifest_set):
        warnings.append(f"manifest: '{texture_id}' is in the catalog but not in tools/textures/build.py")
    for texture_id in sorted(manifest_set - catalog_set):
        warnings.append(f"manifest: '{texture_id}' is in tools/textures/build.py but not the catalog")
    return errors, warnings, report


# ---------------------------------------------------------------------- main


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="validate the shipped PNGs without regenerating them")
    parser.add_argument("--quiet", action="store_true", help="only print problems")
    args = parser.parse_args(argv)

    if not args.check:
        for texture_id in sorted(MANIFEST):
            entry = MANIFEST[texture_id]
            path = os.path.join(ASSET_ROOT, entry["model"])
            canvas = entry["build"]()
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as handle:
                handle.write(write_png(canvas.width, canvas.height, canvas.rgba()))
            if not args.quiet:
                print(f"wrote assets/{entry['model']} ({canvas.width}x{canvas.height})")

    errors, warnings, report = validate_textures()
    if not args.quiet:
        for line in report:
            print(line)
    for warning in warnings:
        print(f"WARN {warning}")
    for error in errors:
        print(f"FAIL {error}")
    if errors:
        print(f"\n{len(errors)} error(s), {len(warnings)} warning(s)")
        return 1
    if not args.quiet:
        print(f"OK ({len(report)} texture(s), {len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

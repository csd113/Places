#!/usr/bin/env python3
"""Surface artwork: glass, polished floors, panels and normal maps.

These are ordinary surface sheets like every other file in this directory: the
game loads the PNGs at level load and never runs this script.  They exist so
the lightweight surface response has real artwork to
demonstrate rather than procedures:

``core:tex_glass_clear_01``
    Nearly transparent sheet: a faint green-grey tint and an alpha channel that
    lets almost everything through.  Tiles, because a window pane still samples
    a material at its own ``tile_metres``.
``core:tex_glass_dirty_01``
    The same sheet with grime: blotchy alpha and a warm-grey film, so a pane
    reads as unwashed rather than as clean glass with a texture on it.
``core:tex_glass_tinted_01``
    A uniformly tinted teal sheet at roughly half opacity.
``core:tex_linoleum_01``
    Polished linoleum: pale speckled sheet with faint seams.
``core:tex_metal_panel_01``
    Brushed metal: cold grey with fine horizontal brushing and a seam.
``core:tex_plastic_panel_01``
    Moulded plastic: pale, near-neutral, with a soft periodic sheen.
``core:tex_normal_panel_01``, ``core:tex_normal_brushed_01``
    Two tangent-space normal maps.  Both are *tileable generic detail*, not
    aligned to any albedo pattern: a panel gets soft moulded dimples, a metal
    sheet fine horizontal brushing.  They are stored as ordinary RGB normals
    (x and y in the red and green channels, z in blue) with a flat alpha, which
    is what the fragment shader decodes.

Painting rules match the rest of the texture set: deterministic helpers only,
no randomness, no clock, no external images, and every pattern period divides
the sheet so the sheet tiles.
"""

from __future__ import annotations

import math

from artkit import Canvas, clamp, fbm, tile_noise, tile_noise2

SIZE = 128

# ------------------------------------------------------------------ palette

GLASS_CLEAR = (218.0, 232.0, 228.0)
GLASS_DIRTY_BASE = (196.0, 203.0, 192.0)
GLASS_DIRTY_FILM = (150.0, 146.0, 126.0)
GLASS_TINTED = (96.0, 148.0, 146.0)

LINOLEUM_BASE = (206.0, 200.0, 178.0)
LINOLEUM_FLECK = (176.0, 170.0, 148.0)
LINOLEUM_SEAM = (150.0, 146.0, 130.0)

METAL_BASE = (112.0, 116.0, 124.0)
METAL_BRUSH = (0.95, 0.97, 1.02)
METAL_SEAM = (104.0, 108.0, 116.0)

PLASTIC_BASE = (206.0, 208.0, 204.0)
PLASTIC_TONE = 0.96


def _wrap(value: int, period: int) -> int:
    """Positive modulo, so wrapped hash lookups stay inside the sheet."""
    return value % period


# --------------------------------------------------------------------- glass


def _build_glass(alpha_base: int, alpha_span: int, tint, film: float, seed: int) -> Canvas:
    """A tiling glass sheet: tint, alpha field and an optional grime film."""
    canvas = Canvas(SIZE, SIZE, (int(tint[0]), int(tint[1]), int(tint[2]), alpha_base))
    for y in range(SIZE):
        for x in range(SIZE):
            # Two scales of blotching: a broad wash plus a finer speckle, both
            # tileable over 64 px so the sheet repeats seamlessly.
            broad = tile_noise(x, y, SIZE, 64, seed)
            fine = tile_noise(x, y, SIZE, 32, seed + 7)
            grime = clamp(broad * 0.7 + fine * 0.3, 0.0, 1.0)
            alpha = int(round(alpha_base + alpha_span * grime))
            colour = (
                tint[0] * (1.0 - film * (0.6 + 0.4 * grime)),
                tint[1] * (1.0 - film * (0.5 + 0.5 * grime)),
                tint[2] * (1.0 - film * (0.8 + 0.2 * grime)),
            )
            canvas.set(x, y, colour, alpha)
    return canvas


def build_glass_clear() -> Canvas:
    # Clear glass is barely there: a 10% sheet with a whisper of variation.
    return _build_glass(alpha_base=24, alpha_span=10, tint=GLASS_CLEAR, film=0.02, seed=31)


def build_glass_dirty() -> Canvas:
    # Unwashed glass: roughly a third opaque, and dirtier where the grime is.
    canvas = _build_glass(
        alpha_base=52, alpha_span=86, tint=GLASS_DIRTY_BASE, film=0.30, seed=53
    )
    # A few dried drips: short vertical trails of thicker film.  They wrap in y,
    # so a trail that reaches the bottom edge continues from the top.
    for drip in range(5):
        x = 11 + drip * 23
        length = 26 + (drip * 17) % 34
        for step in range(length):
            y = _wrap(44 + drip * 13 + step, SIZE)
            film = clamp(1.0 - step / length, 0.0, 1.0)
            index = (y * SIZE + x) * 4
            old = canvas.pixels[index : index + 4]
            canvas.set(
                x,
                y,
                (old[0] * (1.0 - 0.25 * film), old[1] * (1.0 - 0.2 * film), old[2] * (1.0 - 0.3 * film)),
                min(255, int(round(old[3] + 54 * film))),
            )
            canvas.set(
                _wrap(x + 1, SIZE),
                y,
                (old[0] * (1.0 - 0.12 * film), old[1] * (1.0 - 0.1 * film), old[2] * (1.0 - 0.15 * film)),
                min(255, int(round(old[3] + 26 * film))),
            )
    return canvas


def build_glass_tinted() -> Canvas:
    # Tinted sheet: uniform intent, faint rolling variation so it is not flat.
    canvas = Canvas(
        SIZE, SIZE, (int(GLASS_TINTED[0]), int(GLASS_TINTED[1]), int(GLASS_TINTED[2]), 112)
    )
    for y in range(SIZE):
        for x in range(SIZE):
            wash = tile_noise(x, y, SIZE, 64, 71)
            factor = 0.94 + 0.12 * wash
            canvas.set(
                x,
                y,
                (GLASS_TINTED[0] * factor, GLASS_TINTED[1] * factor, GLASS_TINTED[2] * factor),
                112 + int(round(16.0 * wash)),
            )
    return canvas


# ---------------------------------------------------------------- surfaces


def build_linoleum() -> Canvas:
    """Polished linoleum: pale speckled sheet with a faint seam grid."""
    canvas = Canvas(SIZE, SIZE, (int(LINOLEUM_BASE[0]), int(LINOLEUM_BASE[1]), int(LINOLEUM_BASE[2]), 255))
    for y in range(SIZE):
        for x in range(SIZE):
            fine = tile_noise2(x, y, SIZE, 64, 16, 97)
            fleck = 1.0 if fine > 0.72 else 0.0
            wear = fbm(x, y, SIZE, 64, 32, 16, 43)
            colour = (
                LINOLEUM_BASE[0] * (0.94 + 0.08 * wear) - fleck * 26.0,
                LINOLEUM_BASE[1] * (0.94 + 0.08 * wear) - fleck * 24.0,
                LINOLEUM_BASE[2] * (0.94 + 0.08 * wear) - fleck * 22.0,
            )
            canvas.set(x, y, colour, 255)
    # Sheet seams every 64 px, thin and slightly darker: 2 tiles per sheet.
    for offset in (0, 64):
        for x in range(SIZE):
            for y in (offset, _wrap(offset - 1, SIZE)):
                canvas.shade(x, y, 0.86)
        for y in range(SIZE):
            for x in (offset, _wrap(offset - 1, SIZE)):
                canvas.shade(x, y, 0.86)
    return canvas


def build_metal_panel() -> Canvas:
    """Brushed metal: cold grey, horizontal brushing, two rivet rows.

    The brushing varies along rows only, through wrapped tileable noise, so the
    sheet repeats in both directions with no step at the edge. There is
    deliberately no painted seam line: a one-pixel seam cannot wrap, and the
    rivets already carry the "machine panel" reading.
    """
    canvas = Canvas(SIZE, SIZE, (int(METAL_BASE[0]), int(METAL_BASE[1]), int(METAL_BASE[2]), 255))
    for y in range(SIZE):
        # One brush line per row, constant across the row: that is what makes a
        # brushed sheet read as directionally polished rather than noisy.
        line = 0.90 + 0.20 * tile_noise(0, y, SIZE, 64, 17)
        for x in range(SIZE):
            streak = 0.97 + 0.06 * tile_noise(x, y, SIZE, 32, 23)
            factor = line * streak
            canvas.set(
                x,
                y,
                (
                    METAL_BASE[0] * factor * METAL_BRUSH[0],
                    METAL_BASE[1] * factor * METAL_BRUSH[1],
                    METAL_BASE[2] * factor * METAL_BRUSH[2],
                ),
                255,
            )
    for rivet in range(4):
        cx = 16 + rivet * 32
        for dy in range(-2, 3):
            for dx in range(-2, 3):
                if dx * dx + dy * dy <= 5:
                    canvas.shade(_wrap(cx + dx, SIZE), _wrap(8 + dy, SIZE), 1.08)
                    canvas.shade(_wrap(cx + dx, SIZE), _wrap(72 + dy, SIZE), 1.08)
    return canvas


def build_plastic_panel() -> Canvas:
    """Moulded plastic: pale, near-neutral, soft periodic sheen."""
    canvas = Canvas(SIZE, SIZE, (int(PLASTIC_BASE[0]), int(PLASTIC_BASE[1]), int(PLASTIC_BASE[2]), 255))
    for y in range(SIZE):
        for x in range(SIZE):
            mould = tile_noise2(x, y, SIZE, 64, 32, 19)
            colour = (
                PLASTIC_BASE[0] * (PLASTIC_TONE + 0.06 * mould),
                PLASTIC_BASE[1] * (PLASTIC_TONE + 0.06 * mould),
                PLASTIC_BASE[2] * (PLASTIC_TONE + 0.05 * mould),
            )
            canvas.set(x, y, colour, 255)
    return canvas


def build_grille() -> Canvas:
    """A cut-out transfer grille: metal slats over transparent openings.

    The alpha channel *is* the grille: solid where a slat is, zero in the
    openings between them. A material that binds this sheet with
    `alpha_mode: "cutout"` therefore draws real holes instead of a grey
    rectangle, which is what the world's alpha-tested pass exists for.
    """
    slat = 8
    pitch = 16
    # The RGB field is deliberately constant and only the alpha channel carries
    # the pattern: a colour that varied with the slat phase would step at the
    # sheet's edge (row 0 is a highlight, row 127 a shadow), and a cut-out sheet
    # that is sampled with its alpha ignored must still be a sane colour.
    canvas = Canvas(SIZE, SIZE, (128, 132, 138, 0))
    for y in range(SIZE):
        if y % pitch >= slat:
            continue
        for x in range(SIZE):
            canvas.set(x, y, (128.0, 132.0, 138.0), 255)
    return canvas


# -------------------------------------------------------------- normal maps


def _height_field(sample) -> list[float]:
    """A wrapped height field from ``sample(x, y) -> float``."""
    return [sample(x, y) for y in range(SIZE) for x in range(SIZE)]


def _normal_canvas(heights: list[float], strength: float) -> Canvas:
    """Encodes a wrapped height field as a tangent-space normal sheet.

    Central differences on the wrapped grid, scaled by ``strength`` and encoded
    the way the fragment shader decodes: ``(n * 0.5 + 0.5) * 255`` with z
    pointing out of the surface.
    """
    canvas = Canvas(SIZE, SIZE, (128, 128, 255, 255))
    for y in range(SIZE):
        for x in range(SIZE):
            left = heights[y * SIZE + _wrap(x - 1, SIZE)]
            right = heights[y * SIZE + _wrap(x + 1, SIZE)]
            down = heights[_wrap(y - 1, SIZE) * SIZE + x]
            up = heights[_wrap(y + 1, SIZE) * SIZE + x]
            dx = (right - left) * 0.5 * strength
            dy = (up - down) * 0.5 * strength
            length = (dx * dx + dy * dy + 1.0) ** 0.5
            canvas.set(
                x,
                y,
                (
                    (dx / length * 0.5 + 0.5) * 255.0,
                    (dy / length * 0.5 + 0.5) * 255.0,
                    (1.0 / length * 0.5 + 0.5) * 255.0,
                ),
                255,
            )
    return canvas


def build_normal_panel() -> Canvas:
    """Soft moulded dimples: a broad bump field, no hard edges."""
    heights = _height_field(
        lambda x, y: tile_noise(x, y, SIZE, 64, 5) * 0.7
        + tile_noise(x, y, SIZE, 32, 11) * 0.3
    )
    return _normal_canvas(heights, strength=2.4)


def _brush_line(y: int) -> float:
    """A finely grooved height along `y`, exactly periodic over the sheet.

    Three sine harmonics whose periods divide the sheet: the field, and its
    slope, both wrap exactly, so the brushing has no step where the texture
    repeats. (Wrapped value noise cannot give that guarantee for a *directional*
    pattern, whose seams show as a visible line rather than as mild mottling.)
    """
    tau = 2.0 * math.pi
    # Low harmonics only. A normal map's green channel is a *slope*, so its
    # own row-to-row step is a second difference of the height field: a fine
    # groove (a high harmonic) would step twice as hard at the wrap as in the
    # interior and show a seam. Two wide, soft bands of brushing instead read as
    # a satin sheen, tile exactly, and stay honest under the seam metric.
    return (
        0.55 * math.sin(tau * 2.0 * y / SIZE)
        + 0.25 * math.sin(tau * 4.0 * y / SIZE + 1.1)
    )


def build_normal_brushed() -> Canvas:
    """Fine horizontal brushing: periodic grooving along y, constant along x."""
    heights = _height_field(lambda _x, y: _brush_line(y))
    return _normal_canvas(heights, strength=1.7)


# ---------------------------------------------------------------- manifest

ART = {
    "core:tex_glass_clear_01": {
        "model": "core/textures/glass/glass_clear_01.png",
        "build": build_glass_clear,
    },
    "core:tex_glass_dirty_01": {
        "model": "core/textures/glass/glass_dirty_01.png",
        "build": build_glass_dirty,
    },
    "core:tex_glass_tinted_01": {
        "model": "core/textures/glass/glass_tinted_01.png",
        "build": build_glass_tinted,
    },
    "core:tex_linoleum_01": {
        "model": "core/textures/floors/linoleum_01.png",
        "build": build_linoleum,
    },
    "core:tex_metal_panel_01": {
        "model": "core/textures/walls/metal_panel_01.png",
        "build": build_metal_panel,
    },
    "core:tex_plastic_panel_01": {
        "model": "core/textures/walls/plastic_panel_01.png",
        "build": build_plastic_panel,
    },
    "core:tex_grille_01": {
        "model": "core/textures/walls/grille_01.png",
        "build": build_grille,
    },
    "core:tex_normal_panel_01": {
        "model": "core/textures/normals/normal_panel_01.png",
        "build": build_normal_panel,
    },
    "core:tex_normal_brushed_01": {
        "model": "core/textures/normals/normal_brushed_01.png",
        "build": build_normal_brushed,
    },
}

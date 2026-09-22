#!/usr/bin/env python3
"""Diagnostic surface artwork: the architecture-test PNGs.

These sheets are deliberately artificial and orientation-revealing (corner
markers, arrows, rings) and exist to prove arbitrary PNG dimensions (``alt``,
96x64) and alpha decode (``alpha``) on the real renderer.  They are test
fixtures, never shipping level art.
"""

from __future__ import annotations

from artkit import Canvas

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


ART = {
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

#!/usr/bin/env python3
"""Shared 5x7 block font and flat polygon painting.

One canonical source for the block lettering used by the external decal sheets
(``tools/textures/decal_art.py``) and by prop textures
(``tools/props/parts/signage.py``), so a STOP or EXIT sign always draws the same
letterforms.  The helpers are duck-typed over the canvas: they call
``canvas.rect``/``canvas.set``, which both ``tools/textures/artkit.Canvas`` and
``tools/props/tex.Texture`` provide, so no imports cross the two toolkits here.

Glyph layout: seven rows of five bits, most significant bit on the left.
"""

from __future__ import annotations

from typing import List, Tuple

GLYPHS = {
    "N": [
        0b10001,
        0b11001,
        0b10101,
        0b10011,
        0b10001,
        0b10001,
        0b10001,
    ],
    "O": [
        0b01110,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b01110,
    ],
    "D": [
        0b11110,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b11110,
    ],
    "I": [
        0b01110,
        0b00100,
        0b00100,
        0b00100,
        0b00100,
        0b00100,
        0b01110,
    ],
    "V": [
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b10001,
        0b01010,
        0b00100,
    ],
    "G": [
        0b01110,
        0b10001,
        0b10000,
        0b10110,
        0b10001,
        0b10001,
        0b01111,
    ],
    "S": [
        0b01111,
        0b10000,
        0b10000,
        0b01110,
        0b00001,
        0b00001,
        0b11110,
    ],
    "T": [
        0b11111,
        0b00100,
        0b00100,
        0b00100,
        0b00100,
        0b00100,
        0b00100,
    ],
    "P": [
        0b11110,
        0b10001,
        0b10001,
        0b11110,
        0b10000,
        0b10000,
        0b10000,
    ],
    "E": [
        0b11111,
        0b10000,
        0b10000,
        0b11110,
        0b10000,
        0b10000,
        0b11111,
    ],
    "X": [
        0b10001,
        0b10001,
        0b01010,
        0b00100,
        0b01010,
        0b10001,
        0b10001,
    ],
    ">": [
        0b00001,
        0b00011,
        0b00111,
        0b11111,
        0b00111,
        0b00011,
        0b00001,
    ],
}


def stamp_text(canvas, text: str, origin_x: int, origin_y: int, scale: int, rgb) -> int:
    """Draws ``text`` in the block font and returns the width in pixels drawn."""
    cursor = origin_x
    for character in text:
        glyph = GLYPHS.get(character)
        if glyph is not None:
            for row, bits in enumerate(glyph):
                for column in range(5):
                    if bits & (1 << (4 - column)):
                        canvas.rect(
                            cursor + column * scale,
                            origin_y + row * scale,
                            cursor + column * scale + scale - 1,
                            origin_y + row * scale + scale - 1,
                            rgb,
                        )
        cursor += 6 * scale
    return cursor - origin_x - scale


def text_width(text: str, scale: int) -> int:
    return len(text) * 6 * scale - scale


def fill_polygon(canvas, points: List[Tuple[float, float]], rgb, alpha: int = 255) -> None:
    """Scanline-fills a convex polygon with flat colour (integer pixels)."""
    if len(points) < 3:
        return
    top = int(min(y for _, y in points))
    bottom = int(max(y for _, y in points))
    for y in range(top, bottom + 1):
        row_y = y + 0.5
        crossings: list[float] = []
        for index, (x0, y0) in enumerate(points):
            x1, y1 = points[(index + 1) % len(points)]
            if (y0 <= row_y < y1) or (y1 <= row_y < y0):
                crossings.append(x0 + (row_y - y0) / (y1 - y0) * (x1 - x0))
        crossings.sort()
        for pair in range(0, len(crossings) - 1, 2):
            left = int(round(crossings[pair]))
            right = int(round(crossings[pair + 1]))
            for x in range(left, right + 1):
                canvas.set(x, y, rgb, alpha)

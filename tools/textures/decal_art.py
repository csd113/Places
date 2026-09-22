#!/usr/bin/env python3
"""Decal artwork: the final Pool safety-sign sheet.

A decal sheet is an ordinary external PNG like a surface texture, but it is
authored with transparency: the decal pass alpha-tests the sheet, so the
background is alpha 0 and the sign keeps a cut-out silhouette. The runtime must
never regenerate it -- this painter exists so the shipped artwork can be
rebuilt deterministically and so its budget is validated.

Current sheet:

* ``core:decal_no_diving_01`` -- the final pool "NO DIVING" sign: a white plate
  with a red rim, the prohibition pictogram (a diver with the standard red
  circle and slash) and bold block lettering.
"""

from __future__ import annotations

from artkit import Canvas

# ------------------------------------------------------------------- palette

PLATE = (246, 244, 238)
RIM = (188, 46, 40)
INK = (34, 36, 40)
PICTOGRAM = (40, 44, 50)
WATER = (58, 104, 152)
CLEAR = (0, 0, 0, 0)

# ------------------------------------------------------------------- lettering
#
# A tiny 5x7 block font for the handful of letters a sign needs. Each glyph is
# seven rows of five bits, most significant bit on the left.

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
}


def stamp_text(canvas: Canvas, text: str, origin_x: int, origin_y: int, scale: int, rgb) -> int:
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


def fill_polygon(canvas: Canvas, points: list[tuple[float, float]], rgb, alpha: int = 255) -> None:
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


def thick_line(canvas: Canvas, x0: float, y0: float, x1: float, y1: float, width: float, rgb) -> None:
    """A flat-capped line, drawn as a rectangle in the segment's own frame."""
    dx = x1 - x0
    dy = y1 - y0
    length = max(1e-6, (dx * dx + dy * dy) ** 0.5)
    nx = -dy / length * width * 0.5
    ny = dx / length * width * 0.5
    fill_polygon(
        canvas,
        [
            (x0 + nx, y0 + ny),
            (x1 + nx, y1 + ny),
            (x1 - nx, y1 - ny),
            (x0 - nx, y0 - ny),
        ],
        rgb,
    )


# ------------------------------------------------------------------ no diving


def build_no_diving() -> Canvas:
    """The final NO DIVING sign, 128x128 with a transparent background."""
    canvas = Canvas(128, 128, fill=CLEAR)
    # Plate and rim.
    canvas.rect(6, 6, 121, 121, PLATE)
    rim = 5
    canvas.rect(6, 6, 121, 6 + rim - 1, RIM)
    canvas.rect(6, 121 - rim + 1, 121, 121, RIM)
    canvas.rect(6, 6, 6 + rim - 1, 121, RIM)
    canvas.rect(121 - rim + 1, 6, 121, 121, RIM)

    # Prohibition ring over the diving figure.
    ring_centre = (64, 55)
    canvas.disc(ring_centre[0], ring_centre[1], 30, RIM)
    canvas.disc(ring_centre[0], ring_centre[1], 24, PLATE)

    # Diving figure: head, torso, legs and extended arms.
    canvas.disc(70, 38, 5, PICTOGRAM)
    thick_line(canvas, 68, 44, 86, 27, 9.0, PICTOGRAM)  # torso and hips
    thick_line(canvas, 86, 27, 99, 20, 6.0, PICTOGRAM)  # legs
    thick_line(canvas, 86, 27, 98, 31, 5.0, PICTOGRAM)  # trailing leg
    thick_line(canvas, 67, 46, 50, 60, 5.5, PICTOGRAM)  # leading arm
    thick_line(canvas, 69, 50, 55, 66, 4.5, PICTOGRAM)  # trailing arm
    # Water line under the figure.
    thick_line(canvas, 36, 76, 92, 76, 3.0, WATER)

    # The standard prohibition slash, over the figure.
    thick_line(canvas, 39, 84, 89, 27, 8.0, RIM)

    # Lettering.
    label = "NO DIVING"
    scale = 2
    width = text_width(label, scale)
    stamp_text(canvas, label, (128 - width) // 2, 94, scale, INK)
    return canvas


ART = {
    "core:decal_no_diving_01": {
        "model": "environment/pool/decals/no_diving_01.png",
        "build": build_no_diving,
        "kind": "decal",
    },
}

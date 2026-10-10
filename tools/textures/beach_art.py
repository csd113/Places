#!/usr/bin/env python3
"""Beach PNG manifest and deliberate, deterministic offline artwork authoring.

Normal texture builds load the committed PNGs. Only ``--author`` paints new
files; the player never imports or executes this source. ``--check`` compares
every source and derivative with a fresh in-memory export without writing.

Seven concept surface families use 1024-square masters and 512-square runtime
PNGs. The broad shapes deliberately survive the existing 256-pixel Low budget.
The fixed 4x4 prop atlas is 1024-square master -> 256-square Lanczos derivative.
The sky is a seamless 2048x1024 equirectangular PNG, north at U=.5, zenith at V=0.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path
import sys

from artkit import write_png
from home_art import load_sheet

ASSET_ROOT = Path(__file__).resolve().parents[2] / "assets"
ROOT = ASSET_ROOT / "environment/beach"
MASTER_SIZE = 1024
SURFACE_SIZE = 512
NATIVE_SIZE = 256

# Material albedo samples from the board, in sRGB. Illumination is separate.
PALETTE = {
    "sand": (234, 210, 168),
    "grass": (116, 176, 65),
    "rock": (115, 123, 144),
    "stucco": (237, 231, 219),
    "wood": (167, 123, 76),
    "plank": (189, 139, 81),
    "palmtrunk": (162, 119, 69),
    "palmleaf": (93, 151, 52),
    "blue": (50, 138, 202),
    "coral": (244, 103, 83),
    "yellow": (254, 217, 93),
    "ivory": (246, 236, 214),
    "dark": (66, 78, 91),
    "turquoise": (84, 211, 214),
    "window": (145, 204, 217),
    "foam": (250, 245, 232),
}
REGIONS = tuple(PALETTE)

# Sky shader: U=.5+atan2(X,-Z)/tau, V=acos(Y)/pi.
SUN_YAW_DEGREES = 45.0
SUN_ELEVATION_DEGREES = 45.0
SUN_DIRECTION = (0.5, math.sqrt(0.5), -0.5)  # direction toward the sun

SURFACES = {
    "sand_01": ("floors", "sand"),
    "water_01": ("water", "water"),
    "rock_01": ("walls", "rock"),
    "stucco_01": ("walls", "stucco"),
    "wood_plank_01": ("floors", "plank"),
    "palm_trunk_01": ("walls", "palmtrunk"),
    "palm_leaf_01": ("walls", "palmleaf"),
    "grass_01": ("floors", "grass"),
    "roof_yellow_01": ("ceilings", "yellow"),
    "foam_01": ("water", "foam"),
}
ART = {
    f"beach:tex_{name}": {
        "model": f"environment/beach/textures/{directory}/{name}.png",
        "build": lambda model=f"environment/beach/textures/{directory}/{name}.png": load_sheet(model),
    }
    for name, (directory, _) in SURFACES.items()
}
ART["beach:tex_sky_day_01"] = {
    "model": "environment/beach/textures/sky/sky_day_01.png",
    "build": lambda: load_sheet("environment/beach/textures/sky/sky_day_01.png"),
}


def _tone(base, amount):
    return tuple(max(0, min(255, round(channel + amount))) for channel in base)


def _hash(x, y, seed):
    """A fixed integer stream, independent of random-library implementation."""
    value = ((x * 0x9E3779B9) ^ (y * 0x85EBCA6B) ^ seed) & 0xFFFFFFFF
    value ^= value >> 16
    value = (value * 0x7FEB352D) & 0xFFFFFFFF
    value ^= value >> 15
    value = (value * 0x846CA68B) & 0xFFFFFFFF
    value ^= value >> 16
    return (value & 0xFFFFFF) / 0x1000000


def _periodic(image):
    """Join two symmetric edge samples; construction itself is periodic."""
    from PIL import Image

    width, height = image.size
    image.paste(image.crop((0, 0, width, 1)), (0, height - 1))
    image.paste(image.crop((0, 0, 1, height)), (width - 1, 0))
    # Opposite neighbour rows/columns agree as well, keeping filtered joins
    # below the seam gate for low-contrast facets crossing a clipped boundary.
    for axis in (0, 1):
        for band in range(1, 3):
            if axis == 0:
                a = image.crop((band, 0, band + 1, height))
                b = image.crop((width - band - 1, 0, width - band, height))
                blended = Image.blend(a, b, 0.5)
                image.paste(blended, (band, 0))
                image.paste(blended, (width - band - 1, 0))
            else:
                a = image.crop((0, band, width, band + 1))
                b = image.crop((0, height - band - 1, width, height - band))
                blended = Image.blend(a, b, 0.5)
                image.paste(blended, (0, band))
                image.paste(blended, (0, height - band - 1))
    return image


def _clip_polygon(polygon, nx, ny, distance):
    """Clip against nx*x+ny*y <= distance (Voronoi construction)."""
    result = []
    previous = polygon[-1]
    previous_value = previous[0] * nx + previous[1] * ny - distance
    for current in polygon:
        value = current[0] * nx + current[1] * ny - distance
        if (value <= 0) != (previous_value <= 0):
            factor = previous_value / (previous_value - value)
            result.append((previous[0] + (current[0] - previous[0]) * factor,
                           previous[1] + (current[1] - previous[1]) * factor))
        if value <= 0:
            result.append(current)
        previous, previous_value = current, value
    return result


def _cells(size, columns, rows, seed):
    """Periodic irregular polygon cells, with no raster noise or tessellation grid."""
    centers = {}
    for row in range(-2, rows + 2):
        for column in range(-2, columns + 2):
            u, v = column % columns, row % rows
            centers[(column, row)] = (
                (column + 0.24 + _hash(u, v, seed) * 0.52) * size / columns,
                (row + 0.24 + _hash(u, v, seed + 3) * 0.52) * size / rows,
            )
    result = []
    for row in range(-1, rows + 1):
        for column in range(-1, columns + 1):
            center = centers[(column, row)]
            polygon = [(-size, -size), (2 * size, -size),
                       (2 * size, 2 * size), (-size, 2 * size)]
            for dy in range(-1, 2):
                for dx in range(-1, 2):
                    if not dx and not dy:
                        continue
                    neighbour = centers[(column + dx, row + dy)]
                    nx, ny = neighbour[0] - center[0], neighbour[1] - center[1]
                    distance = (neighbour[0] ** 2 + neighbour[1] ** 2
                                - center[0] ** 2 - center[1] ** 2) * 0.5
                    polygon = _clip_polygon(polygon, nx, ny, distance)
            result.append((polygon, center, _hash(column % columns, row % rows, seed + 5)))
    return result


def surface(kind, size=MASTER_SIZE):
    """Author broad material marks; geometric facets supply actual lighting."""
    from PIL import Image, ImageDraw

    base = (41, 184, 204) if kind == "water" else PALETTE[kind]
    image = Image.new("RGBA", (size, size), (*base, 255))
    draw = ImageDraw.Draw(image)
    if kind in ("sand", "water", "rock", "stucco", "grass"):
        columns, rows = {"sand": (5, 8), "water": (7, 7), "rock": (6, 3),
                         "stucco": (8, 8), "grass": (6, 6)}[kind]
        amplitude = {"sand": 11, "water": 17, "rock": 18, "stucco": 4, "grass": 10}[kind]
        cells = _cells(size, columns, rows, 0xBEAC4)
        for polygon, center, variation in cells:
            color = _tone(base, (variation - 0.5) * amplitude * 2)
            draw.polygon(polygon, fill=(*color, 255))
            if kind in ("sand", "rock", "grass"):
                # Sparse additional facets rather than fine speckle.
                triangle = [center, polygon[0], polygon[1]]
                draw.polygon(triangle, fill=(*_tone(color, 4 if variation > 0.5 else -4), 255))
        if kind == "water":
            for polygon, _, _ in cells:
                draw.line(polygon + [polygon[0]], fill=(105, 220, 223, 255),
                          width=max(2, size // 150), joint="curve")
    elif kind in ("wood", "plank", "palmtrunk"):
        # Joins are centered in the canvas rather than on the wrapping edge.
        courses = 8 if kind == "plank" else 10 if kind == "palmtrunk" else 4
        height = size / courses
        for row in range(-1, courses + 1):
            y = (row + 0.5) * height
            amount = (_hash(row % courses, 1, 723) - 0.5) * (22 if kind == "palmtrunk" else 10)
            draw.rectangle((0, y, size, y + height), fill=(*_tone(base, amount), 255))
            if kind in ("plank", "palmtrunk"):
                draw.line((0, y, size, y), fill=(*_tone(base, -22), 255), width=max(1, size // 160))
                draw.line((0, y + size / 160, size, y + size / 160),
                          fill=(*_tone(base, 10), 255), width=max(1, size // 250))
            if kind == "palmtrunk":
                # Ochre bands with staggered broad angular trunk-face marks.
                for column in range(-1, 5):
                    x = (column + (row % 2) * 0.35) * size / 4
                    draw.polygon([(x, y + height * 0.12), (x + size / 7, y + height * 0.2),
                                  (x + size / 9, y + height * 0.87), (x, y + height * 0.78)],
                                 fill=(*_tone(base, amount - 9), 255))
        if kind in ("wood", "plank"):
            for line in range(19):
                y0 = _hash(line, 4, 221) * size
                points = [(x, y0 + math.sin(x / size * math.tau + line) * size / 180)
                          for x in range(0, size + 1, max(1, size // 48))]
                draw.line(points, fill=(*_tone(base, -9 if line % 3 else 9), 255),
                          width=max(1, size // 450))
            # A few simple wood-grain lozenges; no distressed or photorealistic finish.
            for index in range(3):
                x, y = (_hash(index, 2, 721) * size, _hash(index, 5, 733) * size)
                draw.ellipse((x - size / 28, y - size / 140, x + size / 28, y + size / 140),
                             outline=(*_tone(base, -15), 255), width=max(1, size // 350))
    elif kind == "palmleaf":
        # Four broad repeating folded-leaf bands; geometry owns the pointed outline.
        spacing = size / 4
        for band in range(-5, 7):
            start = band * spacing
            draw.polygon([(start, 0), (start + spacing * 0.5, 0),
                          (start + size + spacing * 0.5, size), (start + size, size)],
                         fill=(*_tone(base, 12), 255))
            draw.line((start + spacing * 0.5, 0, start + size + spacing * 0.5, size),
                      fill=(*_tone(base, -8), 255), width=max(1, size // 110))
    elif kind == "yellow":
        # Four broad courses suggest the yellow roof boards in the concept.
        step = size / 4
        for row in range(-1, 5):
            y = (row + 0.5) * step
            draw.line((0, y, size, y), fill=(*_tone(base, -12), 255), width=max(1, size // 180))
            for column in range(-1, 5):
                x = (column + (row % 2) * 0.5 + 0.5) * step
                draw.line((x, y, x, y + step), fill=(*_tone(base, -8), 255), width=max(1, size // 220))
    return _periodic(image)


def atlas():
    from PIL import Image

    image = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE))
    for index, name in enumerate(REGIONS):
        # Flat coloured cloth and glazing remain clean, without painted highlights.
        cell = surface(name, MASTER_SIZE // 4)
        image.paste(cell, ((index % 4) * MASTER_SIZE // 4, (index // 4) * MASTER_SIZE // 4))
    return image


def _cloud(draw, x, y, width, height, seed):
    """Chunky cumulus made of connected angular lobes and a pale-blue bottom."""
    offsets = [(-0.48, 0.1, 0.23), (-0.24, -0.14, 0.27), (0.0, -0.30, 0.30),
               (0.26, -0.12, 0.26), (0.45, 0.04, 0.21)]
    # Connected base hides lobe seams. Angular facets are deliberate sky artwork.
    draw.polygon([(x - width * .56, y + height * .35), (x - width * .5, y),
                  (x + width * .51, y), (x + width * .57, y + height * .24),
                  (x + width * .32, y + height * .46), (x - width * .33, y + height * .47)],
                 fill=(185, 225, 248, 255))
    for index, (dx, dy, radius) in enumerate(offsets):
        cx, cy = x + dx * width, y + dy * height
        rx, ry = radius * width, height * (0.42 + _hash(index, 3, seed) * .12)
        polygon = [(cx + math.cos(i * math.tau / 8 + .15) * rx,
                    cy + math.sin(i * math.tau / 8 + .15) * ry) for i in range(8)]
        color = (239, 249, 255) if index % 3 != 0 else (220, 239, 253)
        draw.polygon(polygon, fill=(*color, 255))
        draw.polygon([(cx, cy), polygon[5], polygon[6], polygon[7]],
                     fill=(249, 253, 255, 255))
        draw.polygon([(cx, cy + ry * .1), polygon[1], polygon[2], polygon[3]],
                     fill=(180, 221, 247, 255))


def sky():
    from PIL import Image, ImageDraw

    width, height = 2048, 1024
    image = Image.new("RGBA", (width, height))
    draw = ImageDraw.Draw(image)
    stops = [(0.0, (35, 105, 211)), (.20, (34, 120, 229)),
             (.35, (43, 158, 242)), (.49, (154, 220, 249)),
             (.53, (182, 231, 247)), (1.0, (140, 205, 223))]
    for y in range(height):
        v = y / (height - 1)
        for index in range(len(stops) - 1):
            if stops[index][0] <= v <= stops[index + 1][0]:
                factor = (v - stops[index][0]) / (stops[index + 1][0] - stops[index][0])
                color = tuple(round(stops[index][1][c] * (1 - factor) + stops[index + 1][1][c] * factor)
                              for c in range(3))
                break
        draw.line((0, y, width - 1, y), fill=(*color, 255))
    clouds = [(20, 395, 190, 43), (230, 315, 225, 73), (460, 413, 140, 31),
              (690, 270, 245, 89), (900, 396, 130, 64), (1090, 432, 170, 47),
              (1460, 332, 230, 74), (1730, 421, 165, 35), (1910, 284, 190, 69),
              (390, 453, 95, 19), (1170, 456, 90, 17), (1610, 454, 100, 20)]
    for index, (x, y, cloud_width, cloud_height) in enumerate(clouds):
        for shift in (-width, 0, width):
            _cloud(draw, x + shift, y, cloud_width, cloud_height, 150 + index)
    # A modest sun disc (~2 degrees) without a large bloom/glare halo.
    sun_x = round((.5 + SUN_YAW_DEGREES / 360) * width)
    sun_y = round((.5 - SUN_ELEVATION_DEGREES / 180) * height)
    draw.ellipse((sun_x - 6, sun_y - 6, sun_x + 6, sun_y + 6), fill=(255, 253, 235, 255))
    # The cloud at the seam is wrapped from the same coordinates and seed.
    image.paste(image.crop((0, 0, 1, height)), (width - 1, 0))
    return image


def outputs():
    """Compute the whole batch before publication; retain every master PNG."""
    from PIL import Image

    result = {}
    for name, (directory, kind) in SURFACES.items():
        master = surface(kind)
        stem = name.removesuffix("_01")
        result[ROOT / f"textures/{directory}/{stem}_master.png"] = master
        result[ROOT / f"textures/{directory}/{name}.png"] = _periodic(
            master.resize((SURFACE_SIZE, SURFACE_SIZE), Image.Resampling.LANCZOS))
    palette_master = atlas()
    result[ROOT / "props/models/beach_palette_master.png"] = palette_master
    # Resize each fixed cell independently so the native atlas has no inter-cell
    # Lanczos ringing. The one-pixel outer gutter repeats its own cell colour.
    native = Image.new("RGBA", (NATIVE_SIZE, NATIVE_SIZE))
    for index in range(16):
        x, y = index % 4 * 256, index // 4 * 256
        cell = palette_master.crop((x, y, x + 256, y + 256)).resize((64, 64), Image.Resampling.LANCZOS)
        native.paste(cell, (index % 4 * 64, index // 4 * 64))
    result[ROOT / "props/models/beach_palette.png"] = native
    result[ROOT / "textures/sky/sky_day_01.png"] = sky()
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--author", action="store_true", help="write deliberate source and runtime PNG artwork")
    mode.add_argument("--check", action="store_true", help="verify byte-identical deterministic output, without writes")
    args = parser.parse_args(argv)
    images = outputs()
    encoded = {path: write_png(image.width, image.height, image.tobytes()) for path, image in images.items()}
    failed = []
    if args.check:
        for path, png in encoded.items():
            if not path.is_file() or path.read_bytes() != png:
                failed.append(str(path.relative_to(ASSET_ROOT)))
    else:
        sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
        from execution import atomic_write

        for path, png in encoded.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            atomic_write(str(path), png)
    if failed:
        print("Beach artwork drift: " + ", ".join(failed), file=sys.stderr)
        return 1
    print(f"Beach artwork {'verified' if args.check else 'authored'}: {len(encoded)} PNGs; "
          "10 surface master/512px pairs, 1024/256px fitted atlas, 2048x1024 sky")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

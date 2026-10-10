#!/usr/bin/env python3
"""Committed Frutiger Aero PNGs and deliberate offline artwork authoring.

Normal texture builds load the committed files through ART; neither the player
nor the normal build path paints replacement imagery. ``--author`` is the
explicit offline export, while ``--check`` reproduces all outputs without
writes. Square periodic surface masters are 1024px and derivatives 512px.
The hand-packed fitted model atlas has a 1024px master and 256px native PNG;
every region retains its exact coordinates. Accent, kiosk and three banner faces
additionally use dedicated 256px atlases with a 128x256 face and stock swatches.
Fitted POT decals carry transparent
margins. The 2048x1024 panorama repeats U, clamps V, and contains no landscape.
"""

from __future__ import annotations

import argparse
import math
from pathlib import Path
import sys

from artkit import write_png
from home_art import load_sheet

ASSET_ROOT = Path(__file__).resolve().parents[2] / "assets"
ROOT = ASSET_ROOT / "environment/frutiger_aero"
MASTER_SIZE = 1024
SURFACE_SIZE = 512
NATIVE_SIZE = 256

# sRGB albedo, sampled from the concept's clean material relationships.
# Highlights, scene light and exposure are intentionally absent from this art.
PALETTE = {
    "white": (245, 240, 234),
    "aqua": (2, 179, 231),
    "cyan": (47, 191, 218),
    "lime": (109, 202, 54),
    "mint": (128, 210, 186),
    "silver": (149, 153, 159),
    "branch": (130, 100, 57),
    "foliage": (88, 145, 48),
    "upholstery": (35, 160, 182),
    "dark": (36, 57, 64),
    "foliage_light": (153, 190, 70),
    "foliage_dark": (56, 113, 50),
    "water": (67, 190, 217),
    "sky": (112, 202, 242),
    "light_cyan": (173, 236, 246),
    "warm_light": (255, 245, 211),
}
REGIONS = tuple(PALETTE)

# Native top-down pixel coordinates, right/bottom exclusive, never repacked.
RECTS = {
    name: ((i % 8) * 32, (i // 8) * 32,
           (i % 8 + 1) * 32, (i // 8 + 1) * 32)
    for i, name in enumerate(REGIONS)
}
RECTS.update({
    "kiosk": (0, 64, 64, 192),
    "atrium": (64, 64, 128, 192),
    "corridor": (128, 64, 192, 192),
    "reception": (192, 64, 256, 192),
    "accent": (0, 192, 32, 256),
    "leaf": (32, 192, 96, 256),
    "cyan_ring": (96, 192, 160, 256),
    "terminal": (160, 192, 256, 256),
})

# Separate native layouts for close readable 1:2 accent and kiosk graphics.
# Shared atlas regions above remain byte-identical and are never repacked.
FOCUS_RECTS = {
    "face": (0, 0, 128, 256),
    "white": (128, 0, 192, 64),
    "lime": (192, 0, 256, 64),
    "cyan": (128, 64, 192, 128),
    "silver": (192, 64, 256, 128),
}
FOCUS_ATLASES = {
    "accent_panel_atlas": "accent",
    "kiosk_atlas": "kiosk",
    "banner_atrium": "atrium",
    "banner_corridor": "corridor",
    "banner_reception": "reception",
}

# Sky shader U=.5+atan2(X,-Z)/tau, V=acos(Y)/pi.
SUN_YAW_DEGREES = 45.0
SUN_ELEVATION_DEGREES = 45.0
SUN_DIRECTION = (0.5, math.sqrt(0.5), -0.5)  # toward the sun

SURFACES = {
    "white_panel_01": ("walls", "white_panel"),
    "aqua_tile_01": ("floors", "aqua_tile"),
    "cyan_glass_01": ("walls", "cyan_glass"),
    "cyan_water_01": ("water", "cyan_water"),
}
DECALS = {
    "atrium_banner_01": "atrium",
    "corridor_typography_01": "corridor",
    "reception_banner_01": "reception",
    "kiosk_typography_01": "kiosk",
    "leaf_brand_01": "leaf",
}
ART = {
    f"frutiger_aero:tex_{name}": {
        "model": f"environment/frutiger_aero/textures/{directory}/{name}.png",
        "build": lambda model=f"environment/frutiger_aero/textures/{directory}/{name}.png": load_sheet(model),
    }
    for name, (directory, _) in SURFACES.items()
}
ART["frutiger_aero:tex_sky_day_01"] = {
    "model": "environment/frutiger_aero/textures/sky/sky_day_01.png",
    "build": lambda: load_sheet("environment/frutiger_aero/textures/sky/sky_day_01.png"),
}
for _name in DECALS:
    _model = f"environment/frutiger_aero/decals/{_name}.png"
    ART[f"frutiger_aero:decal_{_name}"] = {
        "model": _model,
        "build": lambda model=_model: load_sheet(model),
    }

# Project-owned 5x7 uppercase lettering. Deliberately no host font dependency.
GLYPHS = {
    "A": (14, 17, 17, 31, 17, 17, 17),
    "B": (30, 17, 17, 30, 17, 17, 30),
    "C": (14, 17, 16, 16, 16, 17, 14),
    "D": (30, 17, 17, 17, 17, 17, 30),
    "E": (31, 16, 16, 30, 16, 16, 31),
    "G": (14, 17, 16, 23, 17, 17, 14),
    "H": (17, 17, 17, 31, 17, 17, 17),
    "I": (14, 4, 4, 4, 4, 4, 14),
    "L": (16, 16, 16, 16, 16, 16, 31),
    "M": (17, 27, 21, 21, 17, 17, 17),
    "N": (17, 25, 21, 19, 17, 17, 17),
    "O": (14, 17, 17, 17, 17, 17, 14),
    "P": (30, 17, 17, 30, 16, 16, 16),
    "R": (30, 17, 17, 30, 20, 18, 17),
    "S": (15, 16, 16, 14, 1, 1, 30),
    "T": (31, 4, 4, 4, 4, 4, 4),
    "U": (17, 17, 17, 17, 17, 17, 14),
    "W": (17, 17, 17, 21, 21, 21, 10),
    "Y": (17, 17, 10, 4, 4, 4, 4),
}
PHRASES = {
    "kiosk": ("A", "BRIGHTER", "TOMORROW"),
    "atrium": ("NATURE", "PEOPLE", "TECHNOLOGY", "TOGETHER"),
    "corridor": ("CLEANER", "SPACES", "BRIGHTER", "TOMORROWS"),
    "reception": ("PLACES", "A BRIGHTER", "TOMORROW", "TOGETHER"),
}


def _periodic(image):
    """Match filtered neighbouring samples as well as the exact edge texels."""
    from PIL import Image

    width, height = image.size
    for distance in range(3):
        left = image.crop((distance, 0, distance + 1, height))
        right = image.crop((width - distance - 1, 0, width - distance, height))
        column = Image.blend(left, right, 0.5)
        image.paste(column, (distance, 0))
        image.paste(column, (width - distance - 1, 0))
        top = image.crop((0, distance, width, distance + 1))
        bottom = image.crop((0, height - distance - 1, width, height - distance))
        row = Image.blend(top, bottom, 0.5)
        image.paste(row, (0, distance))
        image.paste(row, (0, height - distance - 1))
    return image


def surface(kind, size=MASTER_SIZE):
    """Broad clean periodic material art, with no raster noise or baked light."""
    from PIL import Image, ImageDraw

    if kind == "cyan_glass":
        image = Image.new("RGBA", (size, size))
        pixels = image.load()
        for y in range(size):
            v = y / (size - 1) * math.tau
            for x in range(size):
                u = x / (size - 1) * math.tau
                # Very slight periodic material-density variation. Straight alpha
                # carries 20–25% coverage before an optional material multiplier.
                density = math.cos(u) * math.cos(v)
                pixels[x, y] = (47, 191, 218, round(58 + density * 6))
        return _periodic(image)
    if kind == "cyan_water":
        image = Image.new("RGBA", (size, size), (*PALETTE["water"], 255))
        draw = ImageDraw.Draw(image)
        for row in range(-1, 5):
            for column in range(-1, 5):
                x, y = column * size / 4, row * size / 4
                offset = (row % 2) * size / 8
                points = [(x + offset, y), (x + offset + size / 4, y),
                          (x + offset + size / 3, y + size / 8),
                          (x + offset + size / 6, y + size / 4),
                          (x + offset - size / 12, y + size / 8)]
                color = ((63, 187, 216), (73, 194, 221),
                         (84, 201, 224), (62, 185, 212))[(row + column) % 4]
                draw.polygon(points, fill=(*color, 255))
                draw.line(points + [points[0]], fill=(116, 218, 233, 255),
                          width=max(1, size // 256), joint="curve")
        return _periodic(image)
    image = Image.new("RGBA", (size, size), (*PALETTE["white"], 255))
    draw = ImageDraw.Draw(image)
    count = 2 if kind == "white_panel" else 4
    width = size / count
    white_colors = ((245, 240, 234), (241, 238, 234), (243, 241, 237), (246, 244, 238))
    tiles = (
        (0, 1, 0, 0),
        (1, 0, 0, 2),
        (0, 0, 1, 0),
        (0, 2, 0, 1),
    )
    aqua_colors = ((244, 242, 235), (194, 225, 224), (96, 195, 212))
    for row in range(-1, count + 1):
        for column in range(-1, count + 1):
            # Joints lie in the interior, never at a sample's wrapping edge.
            x, y = (column + 0.5) * width, (row + 0.5) * width
            color = (white_colors[(row + column) % 4] if kind == "white_panel"
                     else aqua_colors[tiles[row % 4][column % 4]])
            draw.rectangle((x, y, x + width, y + width), fill=(*color, 255))
            joint = (224, 226, 223, 255) if kind == "white_panel" else (228, 234, 230, 255)
            draw.line((x, y, x + width, y), fill=joint, width=max(1, size // 170))
            draw.line((x, y, x, y + width), fill=joint, width=max(1, size // 170))
    return _periodic(image)


def _leaf(draw, rect, color, *, vein=None):
    """Angular leaf with a single clean folded plane, matching the concept motif."""
    x0, y0, x1, y1 = rect
    w, h = x1 - x0, y1 - y0
    points = [(x0 + w * .1, y1), (x0, y0 + h * .5),
              (x0 + w * .23, y0 + h * .2), (x1, y0),
              (x0 + w * .8, y0 + h * .65), (x0 + w * .45, y0 + h * .9)]
    draw.polygon(points, fill=color)
    if vein is not None:
        draw.polygon([points[0], points[3], points[4], points[5]], fill=vein)


def _text(draw, text, x, y, scale, color):
    for index, character in enumerate(text):
        if character == " ":
            continue
        if character not in GLYPHS:
            raise ValueError(f"unavailable Aero glyph {character!r}")
        for row, bits in enumerate(GLYPHS[character]):
            for column in range(5):
                if bits & (1 << (4 - column)):
                    px = x + (index * 6 + column) * scale
                    py = y + row * scale
                    draw.rectangle((px, py, px + scale - 1, py + scale - 1), fill=color)


def _graphic(kind, width, height, *, transparent=False):
    from PIL import Image, ImageDraw

    background = (0, 0, 0, 0) if transparent else (*PALETTE["aqua"], 255)
    image = Image.new("RGBA", (width, height), background)
    draw = ImageDraw.Draw(image)
    foreground = (26, 105, 150, 255) if kind == "corridor" else (249, 249, 240, 255)
    if kind == "leaf":
        _leaf(draw, (width * .13, height * .1, width * .88, height * .88),
              (*PALETTE["lime"], 255), vein=(*PALETTE["foliage"], 255))
        return image
    if kind != "corridor":
        leaf_color = (*PALETTE["lime"], 255) if kind == "reception" else foreground
        leaf_fold = (*PALETTE["foliage_light"], 255) if kind == "reception" else (209, 245, 241, 255)
        _leaf(draw, (width * .27, height * .12, width * .73, height * .40),
              leaf_color, vein=leaf_fold)
    lines = PHRASES[kind]
    # Fixed native 5x7 lettering; larger decals use the same exact glyph shapes.
    scale = max(1, min(int(width / ((max(map(len, lines)) * 6 + 5))),
                       int(height / 80)))
    line_step = scale * 12
    start = int(height * (.50 if kind != "corridor" else .26))
    for index, text in enumerate(lines):
        line_width = (len(text) * 6 - 1) * scale
        x = (width - line_width) // 2
        _text(draw, text, x, start + index * line_step, scale, foreground)
    return image


def atlas():
    from PIL import Image, ImageDraw

    image = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE), (*PALETTE["white"], 255))
    factor = MASTER_SIZE // NATIVE_SIZE
    for name, rect in RECTS.items():
        x0, y0, x1, y1 = (value * factor for value in rect)
        width, height = x1 - x0, y1 - y0
        if name in PALETTE:
            cell = Image.new("RGBA", (width, height), (*PALETTE[name], 255))
        elif name in PHRASES or name == "leaf":
            cell = _graphic(name, width, height)
        else:
            cell = Image.new("RGBA", (width, height), (*PALETTE["white"], 255))
            draw = ImageDraw.Draw(cell)
            if name == "accent":
                draw.rectangle((0, 0, width, height), fill=(*PALETTE["aqua"], 255))
                draw.polygon([(0, height * .08), (width, height * .36),
                              (width, height * .75), (0, height * .48)],
                             fill=(*PALETTE["white"], 255))
                _leaf(draw, (width * .08, height * .48, width * .86, height * .78),
                      (*PALETTE["lime"], 255), vein=(*PALETTE["foliage_light"], 255))
                _leaf(draw, (width * .26, height * .76, width * .95, height * .97),
                      (*PALETTE["lime"], 255), vein=(*PALETTE["foliage_light"], 255))
            elif name == "cyan_ring":
                draw.ellipse((width * .05, height * .05, width * .95, height * .95),
                             fill=(*PALETTE["aqua"], 255))
                draw.ellipse((width * .13, height * .13, width * .87, height * .87),
                             fill=(*PALETTE["light_cyan"], 255))
                draw.ellipse((width * .22, height * .22, width * .78, height * .78),
                             fill=(*PALETTE["white"], 255))
            elif name == "terminal":
                draw.rectangle((0, 0, width, height), fill=(*PALETTE["dark"], 255))
                draw.rectangle((width * .08, height * .12, width * .92, height * .86),
                               fill=(41, 104, 114, 255))
                _leaf(draw, (width * .44, height * .29, width * .64, height * .62),
                      (*PALETTE["mint"], 255))
        image.paste(cell, (x0, y0))
    return image


def focused_atlas(kind):
    """Author a full-height 1:2 face without changing the shared fitted atlas."""
    from PIL import Image, ImageDraw

    image = Image.new("RGBA", (MASTER_SIZE, MASTER_SIZE), (*PALETTE["white"], 255))
    width, height = MASTER_SIZE // 2, MASTER_SIZE
    if kind in PHRASES:
        face = _graphic(kind, width, height)
    elif kind == "accent":
        face = Image.new("RGBA", (width, height), (*PALETTE["aqua"], 255))
        draw = ImageDraw.Draw(face)
        # Same fitted concept shapes, newly painted at the larger master size.
        draw.polygon([(0, height * .08), (width, height * .36),
                      (width, height * .75), (0, height * .48)],
                     fill=(*PALETTE["white"], 255))
        _leaf(draw, (width * .08, height * .48, width * .86, height * .78),
              (*PALETTE["lime"], 255), vein=(*PALETTE["foliage_light"], 255))
        _leaf(draw, (width * .26, height * .76, width * .95, height * .97),
              (*PALETTE["lime"], 255), vein=(*PALETTE["foliage_light"], 255))
    else:
        raise ValueError(f"unavailable focused Aero atlas {kind!r}")
    image.paste(face, (0, 0))
    draw = ImageDraw.Draw(image)
    for name, rect in FOCUS_RECTS.items():
        if name != "face":
            x0, y0, x1, y1 = (value * 4 for value in rect)
            draw.rectangle((x0, y0, x1 - 1, y1 - 1), fill=(*PALETTE[name], 255))
    return image


def _cloud(draw, x, y, width, height):
    """Restrained connected low-poly cumulus, without photography or noisy detail."""
    draw.polygon([(x - width * .5, y + height * .2),
                  (x + width * .5, y + height * .2),
                  (x + width * .46, y + height * .50),
                  (x - width * .43, y + height * .47)], fill=(203, 230, 246, 255))
    for i, (dx, dy, radius) in enumerate(((-.33, .1, .21), (-.12, -.11, .27),
                                        (.13, -.17, .29), (.35, .08, .22))):
        cx, cy = x + dx * width, y + dy * height
        points = [(cx + math.cos(j * math.tau / 10) * radius * width,
                   cy + math.sin(j * math.tau / 10) * height * .47) for j in range(10)]
        draw.polygon(points, fill=((244, 251, 255, 255) if i % 2 else (228, 244, 253, 255)))
        draw.polygon([(cx, cy + height * .15), *points[:6]], fill=(210, 235, 250, 255))


def sky():
    from PIL import Image, ImageDraw

    width, height = 2048, 1024
    image = Image.new("RGBA", (width, height))
    draw = ImageDraw.Draw(image)
    stops = ((0.0, (86, 172, 238)), (.22, (102, 190, 245)),
             (.35, (133, 209, 249)), (.5, (199, 237, 253)),
             (.58, (191, 229, 247)), (1.0, (159, 216, 237)))
    for y in range(height):
        v = y / (height - 1)
        for (a, ac), (b, bc) in zip(stops, stops[1:]):
            if a <= v <= b:
                factor = (v - a) / (b - a)
                color = tuple(round(ac[c] * (1 - factor) + bc[c] * factor) for c in range(3))
                draw.line((0, y, width - 1, y), fill=(*color, 255))
                break
    for x, y, cloud_width, cloud_height in ((20, 340, 210, 67), (285, 403, 160, 38),
            (430, 300, 210, 88), (730, 410, 180, 46), (940, 340, 205, 69),
            (1130, 452, 115, 27), (1510, 375, 210, 65), (1770, 430, 190, 39),
            (1920, 267, 180, 79)):
        for shift in (-width, 0, width):
            _cloud(draw, x + shift, y, cloud_width, cloud_height)
    sun_x = round((.5 + SUN_YAW_DEGREES / 360) * width)
    sun_y = round((.5 - SUN_ELEVATION_DEGREES / 180) * height)
    draw.ellipse((sun_x - 6, sun_y - 6, sun_x + 6, sun_y + 6), fill=(255, 254, 236, 255))
    image.paste(image.crop((0, 0, 1, height)), (width - 1, 0))
    return image


def outputs():
    """Build every output in memory before explicit publication."""
    from PIL import Image

    result = {}
    for name, (directory, kind) in SURFACES.items():
        master = surface(kind)
        stem = name.removesuffix("_01")
        result[ROOT / f"textures/{directory}/{stem}_master.png"] = master
        result[ROOT / f"textures/{directory}/{name}.png"] = _periodic(
            master.resize((SURFACE_SIZE, SURFACE_SIZE), Image.Resampling.LANCZOS))
    master = atlas()
    result[ROOT / "props/models/frutiger_aero_atlas_master.png"] = master
    native = Image.new("RGBA", (NATIVE_SIZE, NATIVE_SIZE))
    for rect in RECTS.values():
        # Independent region resampling prevents neighbouring cell colour bleed.
        x0, y0, x1, y1 = rect
        cell = master.crop(tuple(value * 4 for value in rect)).resize(
            (x1 - x0, y1 - y0), Image.Resampling.LANCZOS)
        native.paste(cell, (x0, y0))
    result[ROOT / "props/models/frutiger_aero_atlas.png"] = native
    for name, kind in FOCUS_ATLASES.items():
        focused_master = focused_atlas(kind)
        focused_native = Image.new("RGBA", (NATIVE_SIZE, NATIVE_SIZE), (*PALETTE["white"], 255))
        for rect in FOCUS_RECTS.values():
            x0, y0, x1, y1 = rect
            cell = focused_master.crop(tuple(value * 4 for value in rect)).resize(
                (x1 - x0, y1 - y0), Image.Resampling.LANCZOS)
            focused_native.paste(cell, (x0, y0))
        result[ROOT / f"props/models/{name}_master.png"] = focused_master
        result[ROOT / f"props/models/{name}.png"] = focused_native
    result[ROOT / "textures/sky/sky_day_01.png"] = sky()
    for name, kind in DECALS.items():
        width, height = (512, 512) if kind in ("corridor", "leaf") else (512, 1024)
        result[ROOT / f"decals/{name}.png"] = _graphic(kind, width, height, transparent=True)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--author", action="store_true", help="export deliberate source/runtime PNGs")
    mode.add_argument("--check", action="store_true", help="verify every exact deterministic PNG without writes")
    args = parser.parse_args(argv)
    encoded = {path: write_png(im.width, im.height, im.tobytes()) for path, im in outputs().items()}
    if args.check:
        failed = [str(path.relative_to(ASSET_ROOT)) for path, png in encoded.items()
                  if not path.is_file() or path.read_bytes() != png]
        if failed:
            print("Frutiger Aero artwork drift: " + ", ".join(failed), file=sys.stderr)
            return 1
    else:
        sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
        from execution import atomic_write

        for path, png in encoded.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            atomic_write(str(path), png)
    print(f"Frutiger Aero artwork {'verified' if args.check else 'authored'}: {len(encoded)} PNGs; "
          "4 surface master/512px pairs, 6 fitted 1024/256px atlas pairs, 2048x1024 sky, 5 fitted graphics")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

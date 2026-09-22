#!/usr/bin/env python3
"""Development-only software preview renderer for the prop pack.

Blender is not available in this repository's toolchain, so visual review uses
this tiny z-buffered rasteriser instead.  It renders each shipped GLB with the
prop's own texture and baked vertex colours, using the same unlit
``texture * vertex colour`` model the game's GLES2 shader uses plus a neutral
ground plane, so what you see here is what the handheld shows.

Nothing produced by this script is a runtime asset (thumbnails for the level
editor's prop browser are the one deliberate exception).

Usage (from the Places repository root)::

    python3 tools/props/preview.py --all --out target/prop-previews
    python3 tools/props/preview.py --only core:chair --out target/prop-previews
    python3 tools/props/preview.py --sheet --out target/prop-previews
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
from typing import Dict, List, Sequence

HERE = os.path.dirname(os.path.abspath(__file__))
APP_ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)

import glb  # noqa: E402
from tex import decode_png, write_png  # noqa: E402

ASSET_ROOT = os.path.join(APP_ROOT, "assets")
CATALOG_PATH = os.path.join(ASSET_ROOT, "catalog.json")
PLACEABLE_TYPES = ("prop", "entity")


def _placeable_entries(catalog: dict) -> List[dict]:
    entries = catalog.get("assets")
    if entries is None:
        entries = catalog.get("props", [])
    return [
        entry
        for entry in entries
        if entry.get("asset_type", "prop") in PLACEABLE_TYPES
    ]

BACKGROUND_TOP = (26, 27, 30)
BACKGROUND_BOTTOM = (16, 16, 18)
GROUND = (58, 56, 52)

Vec3 = tuple


class Model:
    def __init__(self) -> None:
        self.positions: List[Vec3] = []
        self.uvs: List[tuple] = []
        self.colors: List[tuple] = []
        self.indices: List[int] = []
        self.texture: tuple[int, int, bytes] = (1, 1, bytes((255, 255, 255, 255)))

    def bounds(self):
        xs = [p[0] for p in self.positions]
        ys = [p[1] for p in self.positions]
        zs = [p[2] for p in self.positions]
        return (min(xs), min(ys), min(zs)), (max(xs), max(ys), max(zs))


def load_model(path: str) -> Model:
    with open(path, "rb") as handle:
        data = handle.read()
    read = glb.read_glb(data)
    model = Model()
    model.positions = [tuple(position) for position in read.positions]
    model.uvs = [tuple(uv) for uv in read.uvs]
    model.colors = [tuple(color) for color in read.colors]
    model.indices = list(read.indices)
    model.texture = decode_png(read.texture_png)
    return model


def project(point, camera) -> tuple:
    """World -> screen (pixels) with a simple perspective camera."""
    px, py, pz = (point[0] - camera["origin"][0], point[1] - camera["origin"][1], point[2] - camera["origin"][2])
    # Right/up/forward basis.
    fx, fy, fz = camera["forward"]
    rx, ry, rz = camera["right"]
    ux, uy, uz = camera["up"]
    view = (px * rx + py * ry + pz * rz, px * ux + py * uy + pz * uz, px * fx + py * fy + pz * fz)
    if view[2] <= 0.05:
        return None
    scale = camera["focal"] / view[2]
    return (
        camera["cx"] + view[0] * scale,
        camera["cy"] - view[1] * scale,
        view[2],
        scale,
    )


def make_camera(low, high, width: int, height: int, direction=(0.85, 0.62, 1.0), fov_degrees: float = 32.0, margin: float = 1.30):
    center = tuple((low[i] + high[i]) * 0.5 for i in range(3))
    # Frame the bounding sphere so every prop fits with the same margin.
    half = [(high[i] - low[i]) * 0.5 for i in range(3)]
    radius = math.sqrt(half[0] ** 2 + half[1] ** 2 + half[2] ** 2)
    distance = (radius * margin) / math.sin(math.radians(fov_degrees) * 0.5)
    norm = math.sqrt(sum(value * value for value in direction))
    direction = tuple(value / norm for value in direction)
    origin = tuple(center[i] + direction[i] * distance for i in range(3))
    forward = tuple(center[i] - origin[i] for i in range(3))
    length = math.sqrt(sum(value * value for value in forward))
    forward = tuple(value / length for value in forward)
    right = (forward[2], 0.0, -forward[0])
    right_length = math.sqrt(sum(value * value for value in right)) or 1.0
    right = tuple(value / right_length for value in right)
    up = (
        right[1] * forward[2] - right[2] * forward[1],
        right[2] * forward[0] - right[0] * forward[2],
        right[0] * forward[1] - right[1] * forward[0],
    )
    up = tuple(-value for value in up)
    focal = (height * 0.5) / math.tan(math.radians(fov_degrees) * 0.5)
    return {
        "origin": origin,
        "forward": forward,
        "right": right,
        "up": up,
        "focal": focal,
        "cx": width * 0.5,
        "cy": height * 0.5,
    }


def render(model: Model, width: int, height: int, direction=(0.85, 0.62, 1.0), ground: bool = True) -> bytes:
    """Renders one model and returns RGBA bytes."""
    low, high = model.bounds()
    camera = make_camera(low, high, width, height, direction=direction)

    # Background gradient.
    pixels = bytearray(width * height * 4)
    for y in range(height):
        t = y / max(1, height - 1)
        color = tuple(int(BACKGROUND_TOP[i] + (BACKGROUND_BOTTOM[i] - BACKGROUND_TOP[i]) * t) for i in range(3))
        row_start = y * width * 4
        for x in range(width):
            index = row_start + x * 4
            pixels[index : index + 3] = bytes(color)
            pixels[index + 3] = 255

    depth = [1e9] * (width * height)
    texture_width, texture_height, texture_pixels = model.texture

    def sample(u: float, v: float):
        x = min(texture_width - 1, max(0, int(u * texture_width)))
        y = min(texture_height - 1, max(0, int(v * texture_height)))
        index = (y * texture_width + x) * 4
        return texture_pixels[index], texture_pixels[index + 1], texture_pixels[index + 2]

    def draw_triangle(a, b, c, color_a, color_b, color_c, uvs, tint=1.0, flat=None):
        sa = project(a, camera)
        sb = project(b, camera)
        sc = project(c, camera)
        if sa is None or sb is None or sc is None:
            return
        area = (sb[0] - sa[0]) * (sc[1] - sa[1]) - (sc[0] - sa[0]) * (sb[1] - sa[1])
        if abs(area) < 1e-9:
            return
        min_x = max(0, int(math.floor(min(sa[0], sb[0], sc[0]))))
        max_x = min(width - 1, int(math.ceil(max(sa[0], sb[0], sc[0]))))
        min_y = max(0, int(math.floor(min(sa[1], sb[1], sc[1]))))
        max_y = min(height - 1, int(math.ceil(max(sa[1], sb[1], sc[1]))))
        for y in range(min_y, max_y + 1):
            for x in range(min_x, max_x + 1):
                px, py = x + 0.5, y + 0.5
                # Barycentric weights via signed edge functions (winding agnostic).
                e0 = (sc[0] - sb[0]) * (py - sb[1]) - (sc[1] - sb[1]) * (px - sb[0])
                e1 = (sa[0] - sc[0]) * (py - sc[1]) - (sa[1] - sc[1]) * (px - sc[0])
                e2 = (sb[0] - sa[0]) * (py - sa[1]) - (sb[1] - sa[1]) * (px - sa[0])
                wa, wb, wc = e0 / area, e1 / area, e2 / area
                if wa < 0.0 or wb < 0.0 or wc < 0.0:
                    continue
                denominator = wa / sa[2] + wb / sb[2] + wc / sc[2]
                if denominator <= 0.0:
                    continue
                z = 1.0 / denominator
                index = y * width + x
                if z >= depth[index]:
                    continue
                depth[index] = z
                u = (wa * uvs[0][0] / sa[2] + wb * uvs[1][0] / sb[2] + wc * uvs[2][0] / sc[2]) / denominator
                v = (wa * uvs[0][1] / sa[2] + wb * uvs[1][1] / sb[2] + wc * uvs[2][1] / sc[2]) / denominator
                tr = (wa * color_a[0] / sa[2] + wb * color_b[0] / sb[2] + wc * color_c[0] / sc[2]) / denominator
                tg = (wa * color_a[1] / sa[2] + wb * color_b[1] / sb[2] + wc * color_c[1] / sc[2]) / denominator
                tb = (wa * color_a[2] / sa[2] + wb * color_b[2] / sb[2] + wc * color_c[2] / sc[2]) / denominator
                texel = sample(max(0.0, min(1.0, u)), max(0.0, min(1.0, v))) if flat is None else flat
                pixel = index * 4
                pixels[pixel] = min(255, int(texel[0] * tr * tint))
                pixels[pixel + 1] = min(255, int(texel[1] * tg * tint))
                pixels[pixel + 2] = min(255, int(texel[2] * tb * tint))
                pixels[pixel + 3] = 255

    if ground:
        pad = max(high[0] - low[0], high[2] - low[2]) * 1.6
        cx = (low[0] + high[0]) * 0.5
        cz = (low[2] + high[2]) * 0.5
        y = -0.002  # a hair below the floor so the contact face never z-fights
        white = (1.0, 1.0, 1.0)
        uvs = [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
        corners = [
            (cx - pad, y, cz - pad),
            (cx + pad, y, cz - pad),
            (cx + pad, y, cz + pad),
            (cx - pad, y, cz + pad),
        ]
        ground_color = tuple(float(channel) for channel in GROUND)
        white_px = (1.0, 1.0, 1.0)
        draw_triangle(corners[0], corners[1], corners[2], white_px, white_px, white_px,
                      (uvs[0], uvs[1], uvs[2]), flat=ground_color)
        draw_triangle(corners[0], corners[2], corners[3], white_px, white_px, white_px,
                      (uvs[0], uvs[2], uvs[3]), flat=ground_color)

    for index in range(0, len(model.indices), 3):
        i0, i1, i2 = model.indices[index], model.indices[index + 1], model.indices[index + 2]
        draw_triangle(
            model.positions[i0],
            model.positions[i1],
            model.positions[i2],
            model.colors[i0],
            model.colors[i1],
            model.colors[i2],
            (model.uvs[i0], model.uvs[i1], model.uvs[i2]),
        )
    return bytes(pixels)


def render_prop_file(path: str, width: int, height: int, direction=(0.85, 0.62, 1.0), ground: bool = True) -> bytes:
    model = load_model(path)
    return render(model, width, height, direction=direction, ground=ground)


def compose_sheet(cells: Sequence[tuple[int, int, bytes]], columns: int, gap: int = 4) -> tuple[int, int, bytes]:
    cell_width = max(cell[0] for cell in cells)
    cell_height = max(cell[1] for cell in cells)
    rows = (len(cells) + columns - 1) // columns
    width = columns * cell_width + (columns + 1) * gap
    height = rows * cell_height + (rows + 1) * gap
    canvas = bytearray(width * height * 4)
    for index in range(width * height):
        canvas[index * 4 : index * 4 + 3] = bytes((32, 32, 36))
        canvas[index * 4 + 3] = 255
    for index, (cell_w, cell_h, pixels) in enumerate(cells):
        column = index % columns
        row = index // columns
        x0 = gap + column * (cell_width + gap)
        y0 = gap + row * (cell_height + gap)
        for y in range(cell_h):
            source = y * cell_w * 4
            destination = ((y0 + y) * width + x0) * 4
            canvas[destination : destination + cell_w * 4] = pixels[source : source + cell_w * 4]
    return width, height, bytes(canvas)


def catalogue_models() -> Dict[str, str]:
    with open(CATALOG_PATH, "r", encoding="utf-8") as handle:
        catalog = json.load(handle)
    return {
        entry["id"]: os.path.join(ASSET_ROOT, entry["model"])
        for entry in _placeable_entries(catalog)
        if entry.get("model")
    }


def catalog_order() -> List[str]:
    with open(CATALOG_PATH, "r", encoding="utf-8") as handle:
        catalog = json.load(handle)
    return [entry["id"] for entry in _placeable_entries(catalog)]


def render_thumbnails(ids: Sequence[str], out_dir: str, size: int = 64) -> None:
    """Small prop-browser thumbnails for the level editor (the only shipped previews)."""
    os.makedirs(out_dir, exist_ok=True)
    models = catalogue_models()
    for prop_id in ids:
        path = models.get(prop_id)
        if not path or not os.path.exists(path):
            continue
        pixels = render_prop_file(path, size, size, direction=(0.9, 0.5, 1.0), ground=False)
        # Transparent background so the browser can show its own swatch colour.
        cleaned = bytearray(pixels)
        for index in range(size * size):
            offset = index * 4
            r, g, b = cleaned[offset], cleaned[offset + 1], cleaned[offset + 2]
            if abs(r - 26) < 4 and abs(g - 27) < 4 and abs(b - 30) < 5:
                cleaned[offset + 3] = 0
        filename = prop_id.split(":")[-1] + ".png"
        with open(os.path.join(out_dir, filename), "wb") as handle:
            handle.write(write_png(size, size, bytes(cleaned)))


def main(argv: List[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--only", nargs="*", default=None, help="prop ids to render")
    parser.add_argument("--all", action="store_true", help="render every catalogue prop")
    parser.add_argument("--sheet", action="store_true", help="write contact sheets instead of single files")
    parser.add_argument("--thumbs", action="store_true", help="write 64x64 level-editor thumbnails")
    parser.add_argument("--out", default=os.path.join(APP_ROOT, "target", "prop-previews"))
    parser.add_argument("--width", type=int, default=240)
    parser.add_argument("--height", type=int, default=180)
    args = parser.parse_args(argv)

    models = catalogue_models()
    order = catalog_order()
    if args.all or not args.only:
        ids = [prop_id for prop_id in order if prop_id in models]
    else:
        ids = [prop_id for prop_id in order if prop_id in args.only]

    if not ids:
        print("no props to render")
        return 1

    os.makedirs(args.out, exist_ok=True)
    if args.thumbs:
        render_thumbnails(ids, os.path.join(APP_ROOT, "level-editor", "assets", "thumbs"))

    cells: List[tuple[int, int, bytes]] = []
    for prop_id in ids:
        path = models[prop_id]
        if not os.path.exists(path):
            print(f"skip {prop_id}: {path} is missing (run tools/props/build.py)")
            continue
        pixels = render_prop_file(path, args.width, args.height)
        if args.sheet:
            cells.append((args.width, args.height, pixels))
        else:
            filename = prop_id.split(":")[-1] + ".png"
            with open(os.path.join(args.out, filename), "wb") as handle:
                handle.write(write_png(args.width, args.height, pixels))
            print(f"rendered {os.path.join(args.out, filename)}")

    if args.sheet and cells:
        for index in range(0, len(cells), 10):
            chunk = cells[index : index + 10]
            width, height, pixels = compose_sheet(chunk, columns=5)
            path = os.path.join(args.out, f"sheet_{index // 10 + 1}.png")
            with open(path, "wb") as handle:
                handle.write(write_png(width, height, pixels))
            print(f"rendered {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

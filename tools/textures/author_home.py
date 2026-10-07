#!/usr/bin/env python3
"""Offline Home concept finishes and fitted atlases; never invoked at runtime.

Ordinary builds load these committed PNGs. Models embed a single 256px native
derivative of the retained 1024px master. No other theme's artwork is authored.
"""
import math
from pathlib import Path
import random

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2] / "assets/environment/home"
SIZE = 1024


def save(im, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    im.convert("RGBA").save(path, optimize=True)


def stock(size, base, seed, amplitude=3):
    rng = random.Random(seed)
    im = Image.new("RGB", (size, size))
    im.putdata([tuple(max(0, min(255, c + rng.randint(-amplitude, amplitude)))
                      for c in base) for _ in range(size * size)])
    return im


def periodic(im):
    # Real image pixels, authored once. Equal boundary texels avoid wrap steps
    # on the coarse grain, including the small repeated wallpaper sprigs.
    for x in range(im.width):
        im.putpixel((x, im.height - 1), im.getpixel((x, 0)))
    for y in range(im.height):
        im.putpixel((im.width - 1, y), im.getpixel((0, y)))
    return im


def wood(size, base, seed, boards=0):
    rng = random.Random(seed)
    im = stock(size, base, seed, 3)
    d = ImageDraw.Draw(im)
    bands = boards or 8
    rowh = size // bands
    for row in range(-1, bands + 1):
        y = row * rowh + rowh // 3
        shift = rng.randint(-8, 8)
        tone = tuple(c + shift for c in base)
        if boards:
            d.rectangle((0, y + 2, size, y + rowh - 2), fill=tone)
            d.line((0, y, size, y), fill=tuple(c - 27 for c in base), width=2)
            # Staggered end joints, and a lighter shoulder to the joint.
            for x in range(-size, size * 2, size // 2):
                xx = x + (row % 2) * size // 4 + size // 8
                d.line((xx, y + 1, xx, y + rowh - 1), fill=tuple(c - 24 for c in base), width=2)
        for j in range(22):
            yy = y + rng.randrange(4, max(5, rowh - 4))
            shade = rng.choice((-12, -7, -3, 4, 7))
            points = [(x, yy + round(2 * math.sin(x * math.tau / size * 2 + j)))
                      for x in range(0, size + 1, max(1, size // 32))]
            d.line(points, fill=tuple(max(0, c + shift + shade) for c in base), width=1)
    return periodic(im)


def tile(base, grout, seed, count):
    im = Image.new("RGB", (SIZE, SIZE), grout)
    d = ImageDraw.Draw(im)
    rng = random.Random(seed)
    cell = SIZE // count
    for row in range(-1, count + 1):
        for col in range(-1, count + 1):
            x, y = col * cell + cell // 3, row * cell + cell // 3
            n = rng.randint(-4, 4)
            d.rectangle((x + 3, y + 3, x + cell - 3, y + cell - 3),
                        fill=tuple(c + n for c in base))
            d.line((x + 4, y + 4, x + cell - 4, y + 4),
                   fill=tuple(min(255, c + n + 5) for c in base), width=2)
            for _ in range(60):
                xx, yy = x + rng.randrange(6, cell - 6), y + rng.randrange(6, cell - 6)
                s = rng.randint(-6, 3)
                d.ellipse((xx, yy, xx + 3, yy + 2), fill=tuple(c + n + s for c in base))
    return periodic(im)


def cloth(size, base, seed, pile=False):
    im = stock(size, base, seed, 5)
    d = ImageDraw.Draw(im)
    rng = random.Random(seed)
    if pile:
        for _ in range(size * size // 22):
            x, y = rng.randrange(size), rng.randrange(size)
            s = rng.choice((-13, -7, 5, 10))
            d.ellipse((x, y, x + rng.randint(2, 5), y + rng.randint(2, 4)),
                      fill=tuple(c + s for c in base))
    else:
        for x in range(0, size, 4):
            d.line((x, 0, x, size), fill=tuple(c - 3 for c in base))
        for y in range(0, size, 4):
            d.line((0, y, size, y), fill=tuple(c + 3 for c in base))
    return periodic(im)


def atlas(name, bases, kinds=None):
    im = Image.new("RGB", (SIZE, SIZE))
    for i, base in enumerate(bases):
        kind = (kinds or {}).get(i)
        cell = (wood(512, base, 110 + i) if kind == "wood" else
                cloth(512, base, 115 + i, pile=kind == "pile") if kind in ("cloth", "pile") else
                stock(512, base, 120 + i, 2))
        im.paste(cell, ((i % 2) * 512, (i // 2) * 512))
    if name == "crt_tv":
        source = ROOT / "props/art/city_dusk_01.png"
        if not source.exists():
            raise RuntimeError("Retain the approved generated city screen PNG first")
        screen = Image.open(source).convert("RGB").resize((504, 380), Image.Resampling.LANCZOS)
        im.paste(screen, (4, 4))
        d = ImageDraw.Draw(im)
        for y in range(4, 384, 5):
            d.line((4, y, 507, y), fill=(79, 75, 101), width=1)
        for x in range(540, 792, 18):
            d.rectangle((x, 725, x + 7, 759), fill=(48, 47, 44))
    if name == "book_stack":
        d = ImageDraw.Draw(im)
        for y in range(530, 1010, 12):
            d.line((520, y, 1010, y), fill=(195, 184, 161), width=2)
    if name == "cushion":
        d = ImageDraw.Draw(im)
        for y in range(0, 512, 80):
            d.rectangle((0, y, 511, y + 27), fill=(132, 119, 97))
        for x in range(0, 512, 80):
            d.rectangle((x, 0, x + 27, 511), fill=(170, 155, 128))
    if name == "landscape_frame":
        d = ImageDraw.Draw(im)
        d.rectangle((8, 8, 503, 503), fill=(160, 178, 177))
        d.polygon([(8, 360), (160, 190), (257, 305), (340, 250), (503, 390), (503, 503), (8, 503)], fill=(106, 128, 123))
        d.polygon([(8, 415), (189, 330), (260, 399), (408, 340), (503, 425), (503, 503), (8, 503)], fill=(89, 103, 77))
        d.ellipse((332, 75, 392, 135), fill=(231, 211, 164))
    save(im, ROOT / f"props/models/{name}_master.png")
    save(im.resize((256, 256), Image.Resampling.LANCZOS), ROOT / f"props/models/{name}.png")


def main():
    for name, base, seed in [("wallpaper_offwhite", (234, 227, 214), 1),
                              ("wall_paint_warm", (238, 232, 221), 2)]:
        save(periodic(stock(SIZE, base, seed, 3)), ROOT / f"textures/walls/{name}_01.png")
    im = stock(SIZE, (234, 227, 213), 4, 2)
    d = ImageDraw.Draw(im)
    for y in range(32, SIZE, 128):
        for x in range(32, SIZE, 128):
            xx = x + (64 if (y // 128) % 2 else 0)
            d.line((xx, y + 22, xx, y + 67), fill=(204, 205, 187), width=4)
            for dx, dy in [(-9, 27), (9, 34), (-10, 43), (10, 51), (0, 18)]:
                d.ellipse((xx + dx - 4, y + dy - 7, xx + dx + 4, y + dy + 3), fill=(201, 203, 184))
    save(periodic(im), ROOT / "textures/walls/wallpaper_pattern_01.png")
    save(wood(SIZE, (172, 131, 88), 5, 8), ROOT / "textures/floors/hardwood_oak_warm_01.png")
    save(wood(SIZE, (118, 84, 59), 6, 8), ROOT / "textures/floors/hardwood_walnut_02.png")
    save(cloth(SIZE, (221, 209, 186), 7, True), ROOT / "textures/floors/carpet_cream_01.png")
    save(tile((225, 217, 201), (187, 182, 171), 8, 4), ROOT / "textures/floors/tile_home_01.png")
    save(tile((231, 224, 210), (190, 186, 175), 9, 8), ROOT / "textures/walls/backsplash_01.png")
    for name, base in [("ceiling_warm", (241, 238, 230)), ("ceiling_plaster_warm", (235, 230, 220))]:
        save(periodic(stock(SIZE, base, 10, 2)), ROOT / f"textures/ceilings/{name}_01.png")
    # Shared sauna/door/wood-trim/fixture art stays intact; profiled Home trim
    # adds real construction depth with those existing file-backed materials.
    sets = {
        "cabinet_base": ([(231, 221, 202), (225, 214, 195), (107, 109, 106), (93, 91, 82)], {}),
        "cabinet_wall": ([(233, 225, 209), (223, 212, 193), (89, 88, 79), (203, 192, 173)], {}),
        "crt_tv": ([(68, 61, 76), (66, 66, 61), (78, 76, 70), (81, 80, 75)], {}),
        "ball_light": ([(247, 231, 190), (75, 72, 64), (173, 154, 123), (239, 226, 193)], {}),
        "sofa": ([(206, 195, 172), (222, 211, 189), (214, 203, 180), (111, 78, 48)], {0:"cloth",1:"cloth",2:"cloth",3:"wood"}),
        "armchair": ([(206, 195, 172), (222, 211, 189), (214, 203, 180), (111, 78, 48)], {0:"cloth",1:"cloth",2:"cloth",3:"wood"}),
        "coffee_table": ([(145, 99, 59), (126, 84, 49), (161, 116, 73), (105, 71, 45)], {0:"wood",1:"wood",2:"wood",3:"wood"}),
        "dining_table": ([(166, 122, 75), (147, 101, 57), (180, 137, 90), (115, 79, 48)], {0:"wood",1:"wood",2:"wood",3:"wood"}),
        "dining_chair": ([(149, 105, 62), (166, 119, 75), (180, 136, 89), (117, 83, 52)], {0:"wood",1:"wood",2:"wood",3:"wood"}),
        "tv_console": ([(143, 99, 59), (128, 85, 49), (97, 70, 45), (71, 76, 68)], {0:"wood",1:"wood",2:"wood"}),
        "bookshelf": ([(144, 102, 64), (131, 88, 51), (111, 71, 40), (161, 121, 78)], {0:"wood",1:"wood",2:"wood"}),
        "rug": ([(227, 215, 193), (214, 201, 176), (207, 194, 169), (221, 209, 187)], {0:"pile",1:"cloth",2:"pile",3:"pile"}),
        "floor_lamp": ([(241, 222, 177), (105, 80, 53), (89, 71, 47), (226, 214, 186)], {}),
        "mug": ([(224, 222, 203), (213, 210, 189), (82, 62, 43), (234, 231, 213)], {}),
        "book_stack": ([(92, 107, 102), (149, 106, 73), (222, 212, 189), (224, 214, 190)], {}),
        "cushion": ([(211, 198, 172), (184, 171, 140), (142, 137, 100), (188, 176, 146)], {0:"cloth",1:"cloth",2:"cloth",3:"cloth"}),
        "outlet": ([(231, 224, 207), (199, 194, 180), (69, 68, 60), (179, 175, 162)], {}),
        "cabinet_strip": ([(231, 219, 185), (250, 230, 187), (113, 110, 97), (188, 185, 171)], {}),
        "landscape_frame": ([(175, 185, 171), (156, 112, 72), (227, 217, 192), (111, 78, 49)], {1:"wood",3:"wood"}),
        "sink": ([(231, 221, 202), (225, 214, 195), (164, 173, 175), (93, 91, 82)], {}),
        "stove": ([(228, 226, 215), (59, 61, 60), (177, 183, 180), (86, 88, 83)], {}),
        "kettle": ([(179, 185, 184), (59, 63, 62), (200, 203, 196), (106, 112, 109)], {}),
        "toaster": ([(219, 211, 190), (52, 56, 55), (184, 188, 178), (92, 98, 95)], {}),
        "door_casing": ([(237, 232, 223), (242, 236, 227), (219, 211, 196), (224, 216, 201)], {}),
    }
    for name, (bases, kinds) in sets.items():
        atlas(name, bases, kinds)


if __name__ == "__main__":
    main()

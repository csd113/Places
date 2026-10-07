#!/usr/bin/env python3
"""Offline Pool concept artwork authoring (Pillow); never called by the game.

Surfaces/fixture faces retain 1024 px masters. Model atlases keep a 1024 px
master beside the GLB and embed only its Lanczos-downsampled 256 px source.
Run deliberately after editing this painter; ordinary builds load the PNGs.
"""
import math
from pathlib import Path
import random

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[2] / "assets/environment/pool"
SIZE = 1024


def save(image, path):
    path.parent.mkdir(parents=True, exist_ok=True)
    image.convert("RGBA").save(path, optimize=True)


def tiles(name, base, grout, seed, speckles=0, count=10):
    rng = random.Random(seed)
    image = Image.new("RGB", (SIZE, SIZE), grout)
    draw = ImageDraw.Draw(image)
    for row in range(count):
        for col in range(count):
            x0, y0 = round(col * SIZE / count), round(row * SIZE / count)
            x1, y1 = round((col + 1) * SIZE / count), round((row + 1) * SIZE / count)
            tone = rng.uniform(-5, 5)
            color = tuple(round(v + tone) for v in base)
            draw.rectangle((x0+2, y0+2, x1-3, y1-3), fill=color)
            draw.line((x0+3, y1-4, x0+3, y0+3, x1-4, y0+3),
                      fill=tuple(min(255, v+8) for v in color), width=1)
            draw.line((x0+4, y1-4, x1-4, y1-4, x1-4, y0+4),
                      fill=tuple(v-8 for v in color), width=1)
            for _ in range(speckles):
                x, y = rng.randrange(x0+6, x1-6), rng.randrange(y0+6, y1-6)
                radius = rng.choice((1, 1, 2, 3))
                shift = rng.randrange(-24, -8) if rng.random() < .7 else 10
                draw.ellipse((x-radius,y-radius,x+radius,y+radius),
                             fill=tuple(max(0,min(255,v+shift)) for v in color))
    # Periodic, low-amplitude glaze mottling: no baked directional lighting.
    pixels = image.load()
    for y in range(SIZE):
        for x in range(SIZE):
            if min(x*count % SIZE, y*count % SIZE) < 24:
                continue
            field = 1.3*math.sin(2*math.pi*x/SIZE*7)*math.sin(2*math.pi*y/SIZE*5)
            pixels[x,y] = tuple(max(0,min(255,round(v+field))) for v in pixels[x,y])
    save(image, ROOT / name)


def atlas(name, colors, seed, kind="resin"):
    rng = random.Random(seed)
    image = Image.new("RGB", (SIZE, SIZE))
    draw = ImageDraw.Draw(image)
    for cell, base in enumerate(colors):
        ox, oy = (cell % 2)*512, (cell // 2)*512
        draw.rectangle((ox,oy,ox+511,oy+511), fill=base)
        if kind == "metal" or (kind == "cloth" and cell > 0):
            for x in range(512):
                factor = 1 + .20*math.exp(-((x/512-.28)/.09)**2) - .18*math.exp(-((x/512-.70)/.12)**2)
                draw.line((ox+x,oy,ox+x,oy+511),fill=tuple(min(255,round(v*factor)) for v in base))
        for _ in range(1200 if kind == "cloth" and cell == 0 else 700):
            x,y = ox+rng.randrange(4,508),oy+rng.randrange(4,508)
            shift = rng.randrange(-4,5)
            draw.line((x,y,x+rng.randrange(1,5),y),fill=tuple(max(0,min(255,v+shift)) for v in base))
        if kind == "cloth" and cell == 0:
            # Fitted vertical cloth UVs: header tape and weighted stitched hem.
            for y in (18,30,480,492):
                draw.line((ox+2,oy+y,ox+509,oy+y),fill=(202,202,193),width=3)
            for x in range(16,512,48):
                draw.rectangle((ox+x,oy+5,ox+x+5,oy+11), fill=(166,171,169))
    master = ROOT / f"props/models/{name}_master.png"
    save(image, master)
    save(image.resize((256,256),Image.Resampling.LANCZOS), ROOT / f"props/models/{name}.png")


def main():
    tiles("textures/floors/pool_tile_deck_01.png", (229,227,218), (173,177,177), 31, 46)
    tiles("textures/floors/pool_tile_basin_01.png", (143,179,205), (195,211,216), 32, 2)
    tiles("textures/walls/pool_tile_wall_01.png", (230,231,226), (195,200,199), 33, 3)
    tiles("textures/ceilings/pool_ceiling_01.png", (222,224,221), (187,191,191), 34, 0, 2)
    tiles("textures/floors/pool_coping_01.png", (229,228,219), (174,180,181), 35, 12, 5)
    tiles("textures/walls/pool_band_01.png", (113,145,163), (160,180,188), 36, 1)
    # Restrained opaque powder-coat swatch, periodic on both axes.
    paint=Image.new("RGB",(SIZE,SIZE)); px=paint.load()
    for y in range(SIZE):
        for x in range(SIZE):
            shift=round(1.2*math.sin(x*math.tau/64)*math.sin(y*math.tau/128))
            px[x,y]=(170+shift,185+shift,191+shift)
    save(paint,ROOT/"textures/walls/pool_metal_01.png")
    for name in ("pool_table","pool_chair"):
        atlas(name,[(242,240,233),(232,231,225),(221,222,216),(236,235,228)],41)
    atlas("pool_ladder",[(184,196,202),(203,211,212),(65,74,77),(123,138,146)],42,"metal")
    for name in ("pool_guardrail_straight","pool_guardrail_end","pool_guardrail_corner"):
        atlas(name,[(183,194,200),(194,204,207),(164,178,183),(91,108,115)],43,"metal")
    for name in ("pool_curtain_straight","pool_curtain_end","pool_curtain_corner"):
        atlas(name,[(234,233,224),(184,195,199),(171,185,190),(203,208,205)],44,"cloth")
    atlas("pool_bench",[(140,151,157),(120,134,141),(181,192,196),(72,85,92)],45)
    atlas("pool_drain",[(192,203,205),(47,65,74),(142,162,169),(228,227,215)],46,"metal")
    atlas("pool_service_door",[(91,124,143),(178,192,197),(218,227,228),(53,75,87)],47)
    # Round opal lens: broad moulding rings remain readable after downsample.
    round_face=Image.new("RGB",(SIZE,SIZE),(182,188,186)); pixels=round_face.load()
    for y in range(SIZE):
        for x in range(SIZE):
            r=math.hypot(x+0.5-512,y+0.5-512)/512
            if r<=1:
                v=238-17*r*r-14*math.exp(-((r-.90)/.02)**2)-5*math.exp(-((r-.67)/.018)**2)
                pixels[x,y]=(round(v),round(v-1),round(v-8))
    save(round_face,ROOT/"textures/lights/pool_light_round_01.png")
    wall_face=Image.new("RGB",(1024,512),(184,192,189)); draw=ImageDraw.Draw(wall_face)
    draw.rectangle((28,24,995,487),fill=(223,226,212))
    for x in range(32,992,24):draw.line((x,28,x,484),fill=(212,219,207),width=4)
    for y in (120,238,356):
        draw.rectangle((36,y,987,y+58),fill=(244,239,219))
        draw.line((40,y+60,984,y+60),fill=(177,188,178),width=3)
    save(wall_face,ROOT/"textures/lights/pool_light_wall_01.png")
    # Navy tile lane stripe, fitted 32:1; real cutout margin on every edge.
    lane=Image.new("RGBA",(1024,32),(0,0,0,0)); draw=ImageDraw.Draw(lane)
    draw.rectangle((1,1,1022,30),fill=(57,91,119,255))
    for x in range(2,1023,9):draw.line((x,1,x,30),fill=(135,161,177,255),width=1)
    save(lane,ROOT/"decals/lane_01.png")
    print("Authored Pool surface/fixture masters and 12 native model atlas sources")


if __name__ == "__main__":
    main()

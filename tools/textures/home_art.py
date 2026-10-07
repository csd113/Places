#!/usr/bin/env python3
"""Home surface manifest: ordinary builds preserve committed PNG artwork.

Deliberate concept authoring lives in author_home.py. Runtime and standard
builds use the real sheets, including unchanged shared trim/door finishes.
"""
from pathlib import Path

from artkit import Canvas
from seam_repair import read_png


def load_sheet(model):
    image = read_png(str(Path(__file__).resolve().parents[2] / "assets" / model))
    canvas = Canvas(image.width, image.height)
    for i in range(image.width * image.height):
        start = i * image.channels
        canvas.pixels[i*4:i*4+3] = image.pixels[start:start+3]
        if image.channels == 4:
            canvas.pixels[i*4+3] = image.pixels[start+3]
    return canvas


SHEETS = {
    "home:tex_wallpaper_offwhite_01": "environment/home/textures/walls/wallpaper_offwhite_01.png",
    "home:tex_wallpaper_pattern_01": "environment/home/textures/walls/wallpaper_pattern_01.png",
    "home:tex_wall_paint_offwhite_01": "environment/home/textures/walls/wall_paint_offwhite_01.png",
    "home:tex_baseboard_wood_01": "environment/home/textures/walls/baseboard_wood_01.png",
    "home:tex_baseboard_white_01": "environment/home/textures/walls/baseboard_white_01.png",
    "home:tex_handrail_wood_01": "environment/home/textures/walls/handrail_wood_01.png",
    "home:tex_threshold_wood_01": "environment/home/textures/floors/threshold_wood_01.png",
    "home:tex_hardwood_oak_01": "environment/home/textures/floors/hardwood_oak_01.png",
    "home:tex_hardwood_walnut_02": "environment/home/textures/floors/hardwood_walnut_02.png",
    "home:tex_carpet_cream_01": "environment/home/textures/floors/carpet_cream_01.png",
    "home:tex_tile_home_01": "environment/home/textures/floors/tile_home_01.png",
    "home:tex_ceiling_white_01": "environment/home/textures/ceilings/ceiling_white_01.png",
    "home:tex_ceiling_plaster_01": "environment/home/textures/ceilings/ceiling_plaster_01.png",
    "home:tex_door_white_01": "environment/home/textures/doors/door_white_01.png",
    "home:tex_sauna_wood_01": "environment/home/textures/doors/sauna_wood_01.png",
    "home:tex_backsplash_01": "environment/home/textures/walls/backsplash_01.png",
    "home:tex_wall_paint_warm_01": "environment/home/textures/walls/wall_paint_warm_01.png",
    "home:tex_hardwood_oak_warm_01": "environment/home/textures/floors/hardwood_oak_warm_01.png",
    "home:tex_ceiling_warm_01": "environment/home/textures/ceilings/ceiling_warm_01.png",
    "home:tex_ceiling_plaster_warm_01": "environment/home/textures/ceilings/ceiling_plaster_warm_01.png",
}

ART = {texture_id: {"model": model, "build": lambda model=model: load_sheet(model)}
       for texture_id, model in SHEETS.items()}

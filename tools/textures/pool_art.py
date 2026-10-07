"""File-backed Pool concept artwork; ordinary builds preserve the masters.

Offline authoring is in author_pool.py. The game loads only committed PNGs.
"""
from pathlib import Path
from artkit import Canvas
from seam_repair import read_png


def load_sheet(folder, name, dimensions=(1024, 1024)):
    path = Path(__file__).resolve().parents[2] / 'assets/environment/pool' / folder / name
    image = read_png(str(path))
    if (image.width,image.height) != dimensions:
        raise ValueError(f'{path}: expected {dimensions}')
    canvas=Canvas(image.width,image.height)
    for i in range(image.width*image.height):
        start=i*image.channels
        canvas.pixels[i*4:i*4+3]=image.pixels[start:start+3]
        if image.channels==4:canvas.pixels[i*4+3]=image.pixels[start+3]
    return canvas


def build_deck(): return load_sheet('textures/floors','pool_tile_deck_01.png')
def build_basin(): return load_sheet('textures/floors','pool_tile_basin_01.png')
def build_wall(): return load_sheet('textures/walls','pool_tile_wall_01.png')
def build_ceiling(): return load_sheet('textures/ceilings','pool_ceiling_01.png')


ART = {
    'core:tex_pool_tile_deck_01': {'model':'environment/pool/textures/floors/pool_tile_deck_01.png','build':build_deck},
    'core:tex_pool_tile_basin_01': {'model':'environment/pool/textures/floors/pool_tile_basin_01.png','build':build_basin},
    'core:tex_pool_tile_wall_01': {'model':'environment/pool/textures/walls/pool_tile_wall_01.png','build':build_wall},
    'core:tex_pool_ceiling_01': {'model':'environment/pool/textures/ceilings/pool_ceiling_01.png','build':build_ceiling},
    'pool:tex_coping_01': {'model':'environment/pool/textures/floors/pool_coping_01.png','build':lambda:load_sheet('textures/floors','pool_coping_01.png')},
    'pool:tex_band_01': {'model':'environment/pool/textures/walls/pool_band_01.png','build':lambda:load_sheet('textures/walls','pool_band_01.png')},
    'pool:tex_metal_01': {'model':'environment/pool/textures/walls/pool_metal_01.png','build':lambda:load_sheet('textures/walls','pool_metal_01.png')},
    'pool:decal_lane_01': {'model':'environment/pool/decals/lane_01.png','kind':'decal','build':lambda:load_sheet('decals','lane_01.png',(1024,32))},
}

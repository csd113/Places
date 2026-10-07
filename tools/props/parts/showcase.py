"""Lantern Hollow's reusable, metre-scale road and woodland modules.

All imagery is loaded from committed, unchanged concrete/fence PNG copies.
Mesh colours describe albedo only: no artificial face lighting or baked AO.
The campfire's emissive meshes animate; its real point light is level-authored.
"""
from __future__ import annotations

import math
from pathlib import Path

from mesh import PropBuilder
from parts.refreshed import load_atlas_from, orient_outward, solid_box, solid_cylinder

MODEL_DIR = Path(__file__).resolve().parents[3] / 'assets/environment/outdoor/props/models'

SIZES = {
    'road_straight': (8.0, .07, 6.0),
    'road_unmarked': (8.0, .064, 6.0),
    'road_dash': (.12, .006, 1.5),
    'sidewalk': (2.4, .14, 6.0),
    'sidewalk_corner': (2.4, .14, 2.4),
    'curb': (.24, .14, 6.0),
    'curb_ramp': (1.2, .14, 2.4),
    'sidewalk_transition': (2.4, .14, 1.2),
    'rock_face': (4.0, 5.0, 2.2),
    'rock_face_variant': (3.4, 4.8, 2.6),
    'boulder': (1.4, .85, 1.3),
    'campfire': (1.3, 1.25, 1.3),
    'road_gate': (8.0, 3.6, .24),
    'stump_seat': (.7, .45, .6),
}


def _atlas(p, wood=False):
    p.ao_strength = 0.0
    name, regions = ('showcase_wood.png', ('wood', 'end')) if wood else (
        'showcase_surface.png', ('body', 'top'))
    return load_atlas_from(p, MODEL_DIR / name, regions)


def _box(p, center, size, uv, color=(255, 255, 255), rotation=None):
    solid_box(p, center, size, uv=uv, color=color, shade=False, rotation=rotation)


def _fit(p):
    """Fit only the authored geometry, preserving the published module sizes."""
    low, high = p.mesh.bounds()
    p.mesh.positions = [
        ((point[0] - (low[0] + high[0]) / 2) * p.width / (high[0] - low[0]),
         (point[1] - low[1]) * p.height / (high[1] - low[1]),
         (point[2] - (low[2] + high[2]) / 2) * p.depth / (high[2] - low[2]))
        for point in p.mesh.positions
    ]


def _closed_rings(p, rings, uv, color):
    """Closed convex faceted solid; one UV seam per face, outward winding."""
    start = len(p.mesh.indices)
    count = len(rings[0])
    for lower, upper in zip(rings, rings[1:]):
        for index in range(count):
            nxt = (index + 1) % count
            p.mesh.quad(lower[index], lower[nxt], upper[nxt], upper[index],
                        uv=uv, color=color)
    for ring in (rings[0], rings[-1]):
        center = tuple(sum(point[axis] for point in ring) / count for axis in range(3))
        for index in range(count):
            p.mesh.triangle(center, ring[index], ring[(index + 1) % count],
                            uvs=[((uv[0] + uv[2]) / 2, (uv[1] + uv[3]) / 2),
                                 (uv[0], uv[3]), (uv[2], uv[3])], color=color)
    center = tuple(sum(point[axis] for ring in rings for point in ring) /
                   (count * len(rings)) for axis in range(3))
    orient_outward(p.mesh, start, center)


def _slab(p, color=(238, 236, 229), height=None, chamfer=.025):
    tex = _atlas(p)
    w, h, d = p.size
    h = h if height is None else height
    bevel = min(chamfer, h * .32)
    rings = []
    for y, inset in ((0, 0), (h - bevel, 0), (h, bevel)):
        rings.append([(-w/2+inset, y, -d/2+inset), (w/2-inset, y, -d/2+inset),
                      (w/2-inset, y, d/2-inset), (-w/2+inset, y, d/2-inset)])
    _closed_rings(p, rings, tex.uv('top', inset=2), color)
    return tex


def build_road_unmarked(p):
    _slab(p, color=(57, 60, 66), chamfer=.006)
    p.add_note('8 m road width; x edges join curb modules; y=.064 top')


def build_road_straight(p):
    tex = _slab(p, color=(57, 60, 66), height=.064, chamfer=.006)
    for z in (-1.5, 1.5):
        _box(p, (0, .067, z), (.12, .006, 1.5), tex.uv('top', inset=2),
             color=(255, 211, 108))
    p.add_note('two raised yellow centre dashes; separate road_dash is available')


def build_road_dash(p):
    tex = _atlas(p)
    _box(p, (0, p.height/2, 0), p.size, tex.uv('top', inset=2),
         color=(255, 211, 108))


def build_sidewalk(p):
    _slab(p, chamfer=.018)
    p.add_note('closed 14 cm slab; no overlapping surface overlays')


def build_curb_ramp(p):
    tex = _atlas(p)
    w, h, d = p.size
    # A closed wedge with a 1 mm low nose, below the engine step threshold.
    rings = [[(-w/2, 0, -d/2), (w/2, 0, -d/2), (w/2, 0, d/2), (-w/2, 0, d/2)],
             [(-w/2, h, -d/2), (w/2, h, -d/2), (w/2, .001, d/2), (-w/2, .001, d/2)]]
    _closed_rings(p, rings, tex.uv('top', inset=2), (238, 236, 229))
    p.add_note('rises towards local -Z; use authored floor/ramp collision alongside')


def _rock(p, variant=0, boulder=False):
    tex = _atlas(p)
    n = 12 if boulder else 8
    rings = []
    for level, (y, radial) in enumerate(((0, .98), (.42, 1), (1, .76))):
        ring = []
        for i in range(n):
            angle = math.tau * i / n
            ripple = 1 + .075 * math.sin(i * 2.3 + variant * 1.7 + level)
            height = y if level == 0 else y + .035 * math.sin(i * 1.6 + variant)
            ring.append((math.cos(angle) * radial * ripple,
                         height, math.sin(angle) * radial * ripple))
        rings.append(ring)
    _closed_rings(p, rings, tex.uv('body', inset=2),
                  (188, 192, 199) if boulder else (156, 162, 170))
    _fit(p)
    p.add_note('closed, faceted rock; outward triangle normals define real surface lighting')


def build_rock_face(p):
    _rock(p)


def build_rock_face_variant(p):
    _rock(p, variant=1)


def build_boulder(p):
    _rock(p, variant=2, boulder=True)


def build_stump_seat(p):
    tex = _atlas(p, wood=True)
    n = 12
    rings = []
    for level, (y, radial) in enumerate(((0, 1), (.08, .84), (.40, .82), (.45, .86))):
        rings.append([(math.cos(math.tau*i/n) * radial * (.35 + .014*math.sin(i*2.4)),
                       y, math.sin(math.tau*i/n) * radial * .30) for i in range(n)])
    _closed_rings(p, rings, tex.uv('wood', inset=2), (255, 255, 255))
    # Map the level seat's cap to the existing end-grain half of the atlas.
    start = len(p.mesh.indices) - n * 3
    uv = tex.uv('end', inset=2)
    for index in p.mesh.indices[start:]:
        x, _, z = p.mesh.positions[index]
        p.mesh.uvs[index] = (uv[0] + (.5 + x/.7) * (uv[2] - uv[0]),
                           uv[1] + (.5 + z/.6) * (uv[3] - uv[1]))
    _fit(p)
    p.add_note('45 cm seat height; end-grain cap, tapered bark/root silhouette')


def build_road_gate(p):
    tex = _atlas(p, wood=True)
    uv = tex.uv('wood', inset=2)
    # A tall closed timber gate with heavy posts, rails and diagonal braces.
    for x in (-3.88, 3.88):
        _box(p, (x, 1.8, 0), (.24, 3.6, .24), uv)
    for x in (-1.91, 1.91):
        for y in (.40, 1.65, 2.90):
            _box(p, (x, y, -.075), (3.56, .15, .09), uv)
        for i in range(19):
            bx = x + (i - 9) * .18
            _box(p, (bx, 1.68, .015), (.16, 3.28, .10), uv)
        for direction in (-1, 1):
            _box(p, (x, 1.65, -.10), (3.98, .12, .04), uv,
                 rotation=(0, 0, direction * 39))
    _fit(p)
    p.add_note('8 m road-end barrier; level authors a matching solid collider')


def build_campfire(p):
    # Preserve this authored GLB's original material JSON byte ordering.
    p.mesh.texture_first_materials = True
    tex = _atlas(p)
    uv = tex.uv('body', inset=2)
    stone = p.material('campfire_stone')
    timber = p.material('campfire_charred_logs')
    ember = p.material('campfire_embers', emissive=(1.0, .23, .025), strength=1.7)
    flame = p.material('campfire_flame', emissive=(1.0, .56, .055), strength=2.2)
    p.begin_mesh('campfire_base')
    p.begin_material(stone)
    for i in range(12):
        a = math.tau * i / 12
        base = (.515*math.cos(a), 0, .515*math.sin(a))
        solid_cylinder(p, base, .14, .22 + .02*math.sin(i*2), segments=8,
                       uv=uv, color=(185, 185, 180), taper=.76, shades=False)
    p.begin_material(timber)
    for i in range(4):
        a = math.tau * i / 4 + .40
        start = (-.38*math.cos(a), .17+.035*i, -.38*math.sin(a))
        end = (.38*math.cos(a), .26+.02*i, .38*math.sin(a))
        # Mesh.tube's historic face shading is neutralized after geometry.
        first_vertex = len(p.mesh.positions)
        p.tube(start, end, .067, segments=8, uv=uv, color=(83, 48, 28))
        p.mesh.colors[first_vertex:] = [(83, 48, 28)] * (len(p.mesh.positions) - first_vertex)
    p.begin_material(ember)
    solid_cylinder(p, (0, .07, 0), .31, .03, segments=8, uv=uv,
                   color=(255, 105, 21), shades=False)
    root = p.node('campfire_root', mesh='campfire_base')
    pivots = []
    channels = []
    times = [0, .25, .5, .75, 1, 1.25]
    for i in range(5):
        p.begin_mesh(f'campfire_flame_{i}')
        p.begin_material(flame)
        a = math.tau * i / 5
        center = (math.cos(a)*.12, .24, math.sin(a)*.12)
        height = (1.01, .75, .86, .69, .79)[i]
        profile = [(0, .13), (height*.26, .19), (height*.63, .095), (height, .004)]
        start = len(p.mesh.indices)
        p.lathe(center, profile, segments=5, uv=tex.uv('top', inset=2),
                color=(255, 204, 92), shades=False)
        orient_outward(p.mesh, start, (center[0], center[1]+height*.35, center[2]))
        pivot = p.node(f'flame_pivot_{i}')
        node = p.node(f'flame_node_{i}', mesh=f'campfire_flame_{i}')
        p.nodes[pivot]['children'] = [node]
        pivots.append(pivot)
        factors = [1, .77+.02*i, .91-.02*i, .73+.03*i, .94-.01*i, 1]
        channels.append({'node': node, 'path': 'scale', 'times': times,
                         'values': [(1-.08*(1-f), f, 1-.08*(1-f)) for f in factors],
                         'interpolation': 'LINEAR'})
    p.nodes[root]['children'] = pivots
    p.clip('flicker', channels)
    _fit(p)
    p.add_note('five rigid flames, flicker clip 1.25 s; no smoke billboard')
    p.add_note('real level point emitter: offset [0,1.48,0], range 9 m; emission alone lights nothing')


PROPS = {
    'outdoor:showcase_road_straight': build_road_straight,
    'outdoor:showcase_road_unmarked': build_road_unmarked,
    'outdoor:showcase_road_dash': build_road_dash,
    'outdoor:showcase_sidewalk': build_sidewalk,
    'outdoor:showcase_sidewalk_corner': build_sidewalk,
    'outdoor:showcase_curb': build_sidewalk,
    'outdoor:showcase_curb_ramp': build_curb_ramp,
    'outdoor:showcase_sidewalk_transition': build_sidewalk,
    'outdoor:showcase_rock_face': build_rock_face,
    'outdoor:showcase_rock_face_variant': build_rock_face_variant,
    'outdoor:showcase_boulder': build_boulder,
    'outdoor:showcase_campfire': build_campfire,
    'outdoor:showcase_road_gate': build_road_gate,
    'outdoor:showcase_stump_seat': build_stump_seat,
}

"""File-backed winter kit. Geometry only; existing artwork is never repainted.

Canonical trees, rocks and rails are imported from their shipped GLBs without
moving, refitting, recolouring or remapping a single base vertex. Closed snow
shells are a separate named mesh/material. Modular additions are non-solid;
the level retains each structure's original collider.
"""
from __future__ import annotations

from functools import partial
import math
from pathlib import Path
import random

import glb
from parts.refreshed import load_atlas_from

ROOT = Path(__file__).resolve().parents[3]
OUTDOOR = ROOT / 'assets/environment/outdoor/props/models'
WINTER = ROOT / 'assets/environment/winter/props/models'
SNOW = (244, 248, 255)
ICE = (193, 218, 243)

SIZES = {
    'tree_snow_01': (3.2, 6.8, 3.2),
    'tree_snow_02': (3.2, 6.8, 3.2),
    'drift_small': (1.2, .28, .8),
    'drift_medium': (2.4, .55, 1.5),
    'drift_large': (4.4, .9, 2.8),
    'drift_wall': (3, .65, 1.1),
    'drift_fence': (1.8, .38, .7),
    'mound_irregular': (2.1, .75, 1.9),
    'boulder_snow': (1.4, .97, 1.3),
    'rock_face_snow': (4, 5.16, 2.2),
    'snow_door_overhang': (1.6, .14, .55),
    'snow_awning': (2.4, .18, 2.6),
    'snow_stair_edge': (.38, .085, .8),
    'snow_ledge': (1.2, .10, .24),
    'snow_roof_edge': (2.4, .16, .36),
    'snow_porch_edge': (1.8, .10, .3),
    'snow_roof_slope': (3.4, 1.41, 2),
    'snow_rail_top': (1.2, .075, .11),
    'snow_post_cap': (.12, .085, .12),
    'railing_snow_straight': (1.8, 1.125, .16),
    'railing_snow_corner': (.3, 1.125, .3),
    'railing_snow_end': (.3, 1.135, .16),
    'fence_post_snow': (.16, 1.135, .16),
    'icicle_short': (.07, .18, .07),
    'icicle_medium': (.10, .36, .10),
    'icicle_long': (.13, .65, .13),
    'icicle_cluster_mixed': (1.2, .65, .13),
    'icicle_cluster_sparse': (1.2, .36, .10),
}


def _normal(a, b, c):
    u, v = [b[i] - a[i] for i in range(3)], [c[i] - a[i] for i in range(3)]
    return (u[1]*v[2] - u[2]*v[1], u[2]*v[0] - u[0]*v[2], u[0]*v[1] - u[1]*v[0])


def _triangles(model, material=None):
    for face, start in enumerate(range(0, len(model.indices), 3)):
        if material is None or model.triangle_materials[face] == material:
            yield tuple(model.positions[i] for i in model.indices[start:start + 3])


def surface_height(triangles, x, z):
    """Highest supporting opaque triangle, with a vertical barycentric probe."""
    heights = []
    for a, b, c in triangles:
        den = (b[2]-c[2])*(a[0]-c[0]) + (c[0]-b[0])*(a[2]-c[2])
        if abs(den) < 1e-9:
            continue
        u = ((b[2]-c[2])*(x-c[0]) + (c[0]-b[0])*(z-c[2])) / den
        v = ((c[2]-a[2])*(x-c[0]) + (a[0]-c[0])*(z-c[2])) / den
        if min(u, v, 1-u-v) >= -1e-6:
            heights.append(u*a[1] + v*b[1] + (1-u-v)*c[1])
    return max(heights) if heights else None


def _canonical(p, name):
    model = glb.read_glb((OUTDOOR / (name + '.glb')).read_bytes())
    source = name if (OUTDOOR / (name + '.png')).is_file() else 'showcase_surface'
    load_atlas_from(p, OUTDOOR / (source + '.png'), (), keep_alpha=True)
    p.ao_strength = 0
    p.begin_mesh('canonical_base')
    for material in model.json['materials']:
        p.material(material['name'], alpha_mode='mask' if material.get('alphaMode') == 'MASK' else None,
                   alpha_cutoff=material.get('alphaCutoff'))
    # Preserve the original vertex records and triangle order verbatim.
    p.mesh.positions = list(model.positions)
    p.mesh.uvs = list(model.uvs)
    p.mesh.colors = [tuple(round(channel*255) for channel in color[:3]) for color in model.colors]
    p.mesh.indices = list(model.indices)
    p.mesh._triangle_materials = list(model.triangle_materials)
    p.begin_mesh('snow_accumulation')
    p.begin_material(p.material('snow', use_texture=False))
    return model


def _snow(p):
    p.ao_strength = 0
    load_atlas_from(p, WINTER / 'snow_surface.png', ())
    p.begin_material(p.material('snow'))


def _face(p, points, center, color=SNOW):
    """Closed-component winding, with continuous planar UVs on snow tops."""
    uv = [(max(0, min(1, x / p.width + .5)), max(0, min(1, z / p.depth + .5)))
          for x, _, z in points]
    n = _normal(*points[:3])
    if sum(n[i]*(points[0][i]-center[i]) for i in range(3)) < 0:
        points, uv = list(reversed(points)), list(reversed(uv))
    if len(points) == 3:
        p.mesh.triangle(*points, uvs=uv, color=color)
    else:
        p.mesh.quad(*points, uv=uv, color=color)


def _shell(p, base, top, color=SNOW):
    """A shallow, closed convex snow clump seated into its support plane."""
    # Thin, nonplanar caps cannot use a centre-dot winding heuristic: the
    # centre can lie above a low top facet. Orient the rings topologically.
    if _normal(*base[:3])[1] < 0:
        base, top = list(reversed(base)), list(reversed(top))
    def emit(points):
        uv = [(max(0, min(1, x/p.width+.5)), max(0, min(1, z/p.depth+.5)))
              for x, _, z in points]
        if len(points) == 3:
            p.mesh.triangle(*points, uvs=uv, color=color)
        else:
            p.mesh.quad(*points, uv=uv, color=color)
    for i in range(len(base)):
        j = (i + 1) % len(base)
        emit([base[i], base[j], top[j], top[i]])
    for i in range(1, len(base) - 1):
        emit([base[0], base[i+1], base[i]])
        emit([top[0], top[i], top[i+1]])


def build_tree(p, heavy=False):
    model = _canonical(p, 'tree_03')
    foliage = list(_triangles(model, 1))
    # The canonical builder emits independent closed six-sided bough lobes.
    # Discover their connectivity from welded positions, so no triangle count
    # or vertex order is assumed. Coat their connected upper surfaces once.
    from collections import defaultdict
    adjacent = defaultdict(set)
    for i, triangle in enumerate(foliage):
        for point in triangle:
            adjacent[point].add(i)
    seen = set()
    groups = []
    for start in range(len(foliage)):
        if start in seen:
            continue
        group, pending = set(), [start]
        while pending:
            face = pending.pop()
            if face in group:
                continue
            group.add(face)
            for point in foliage[face]:
                pending.extend(adjacent[point] - group)
        seen.update(group)
        groups.append([foliage[i] for i in sorted(group)])
    for i, group in enumerate(groups):
        if (heavy and i % 4 == 2) or (not heavy and (i % 3 == 1 or i == 3)):
            continue
        upper = [t for t in group if _normal(*t)[1] > 1e-7]
        edges = defaultdict(list)
        thickness = .125 if heavy else .080
        def bottom(q):
            return (q[0], q[1]-.008, q[2])
        def top(q):
            return (q[0], min(p.height, q[1]+thickness*(.86+.12*math.sin(q[0]*4+q[2]*3+i)**2)), q[2])
        def snow_uv(q):
            return (max(0, min(1, q[0]/p.width+.5)), max(0, min(1, q[2]/p.depth+.5)))
        for a, b, c in upper:
            uv = [snow_uv(q) for q in (a, b, c)]
            p.mesh.triangle(top(a), top(b), top(c), uvs=uv, color=SNOW)
            p.mesh.triangle(bottom(c), bottom(b), bottom(a), uvs=list(reversed(uv)), color=SNOW)
            for a, b in ((a, b), (b, c), (c, a)):
                edges[tuple(sorted((a, b)))].append((a, b))
        for uses in edges.values():
            if len(uses) == 1:
                a, b = uses[0]
                uv = [snow_uv(q) for q in (a, a, b, b)]
                p.mesh.quad(bottom(a), bottom(b), top(b), top(a), uv=uv, color=SNOW)
    p.add_note('current canonical 708-triangle evergreen; connected supported upper-bough loads, dark undersides and sealed snow rims')


def build_rock(p, cliff=False):
    model = _canonical(p, 'showcase_rock_face' if cliff else 'showcase_boulder')
    triangles = list(_triangles(model))
    height = max(v[1] for t in triangles for v in t)
    # Convex X/Z hull of the upper shoulder, rather than joining vertices from
    # several different rings into a self-intersecting cap.
    points = sorted({(x, z) for x, y, z in model.positions if y >= height*.52})
    def turn(a, b, c):
        return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
    lower, upper = [], []
    for seq, hull in ((points, lower), (reversed(points), upper)):
        for q in seq:
            while len(hull) >= 2 and turn(hull[-2], hull[-1], q) <= 0:
                hull.pop()
            hull.append(q)
    hull = lower[:-1]+upper[:-1]
    cx, cz = (sum(q[k] for q in hull)/len(hull) for k in (0, 1))
    ring = [(cx+(x-cx)*.985, cz+(z-cz)*.985) for x, z in hull]
    base = [(x, surface_height(triangles, x, z)-.009, z) for x, z in ring]
    cap = .16 if cliff else .12
    top = [(x, y+.009+cap*(.63+.14*math.sin(i*2.7)**2), z) for i, (x, y, z) in enumerate(base)]
    if _normal(*base[:3])[1] < 0:
        base, top = list(reversed(base)), list(reversed(top))
    foot = (cx, surface_height(triangles, cx, cz)-.009, cz)
    crown = (cx, height+cap, cz)
    for i in range(len(base)):
        j = (i+1) % len(base)
        uv = [(q[0]/p.width+.5, q[2]/p.depth+.5) for q in (base[i], base[j], top[j], top[i])]
        p.mesh.quad(base[i], base[j], top[j], top[i], uv=uv, color=SNOW)
        p.mesh.triangle(foot, base[j], base[i], uvs=[(.5,.5),uv[1],uv[0]], color=SNOW)
        p.mesh.triangle(crown, top[i], top[j], uvs=[(.5,.5),uv[3],uv[2]], color=SNOW)
    p.add_note('current canonical dark rock retained; closed continuous snow shoulder/crown with exposed lower mineral faces')


def _fit(p):
    low, high = p.mesh.bounds()
    p.mesh.positions = [((v[0]-(low[0]+high[0])/2)*p.width/(high[0]-low[0]),
                         (v[1]-low[1])*p.height/(high[1]-low[1]),
                         (v[2]-(low[2]+high[2])/2)*p.depth/(high[2]-low[2]))
                        for v in p.mesh.positions]


def build_drift(p, wall=False):
    _snow(p)
    count = 10
    seed = sum(ord(c) for c in p.id)*.031
    rings = []
    for radial, height in ((1, 0), (.72, .38), (.34, .83)):
        ring = []
        for i in range(count):
            angle = math.tau*i/count
            ripple = 1 + .16*math.sin(i*2.3 + height*3 + seed)
            x, z = math.cos(angle)*radial*ripple, math.sin(angle)*radial*ripple
            if wall:
                z = max(z, -.45)  # Flatten the back against a wall/fence.
            ring.append((x, height*(.82 + .18*math.sin(i*1.8+seed)**2), z))
        rings.append(ring)
    center = (.08, .4, .02)
    for lower, upper in zip(rings, rings[1:]):
        for i in range(count):
            j = (i + 1) % count
            _face(p, [lower[i], lower[j], upper[j], upper[i]], center)
    for i in range(count):
        j = (i + 1) % count
        _face(p, [(0, 0, 0), rings[0][j], rings[0][i]], center)
        _face(p, [(.18*math.sin(seed), 1, -.12*math.cos(seed)), rings[-1][i], rings[-1][j]], center)
    _fit(p)
    # Recompute UVs after fitting so every neighbouring top facet agrees.
    p.mesh.uvs = [(x/p.width+.5, z/p.depth+.5) for x, _, z in p.mesh.positions]
    p.add_note('60 triangles; unequal wind-shaped shoulders per family, closed grounded drift; sink base 8 mm')


def _strip(p, width, height, depth, *, x=0, y=0, z=0, slope=0, seed=33):
    rng = random.Random(seed)
    # One irregular ridge avoids repeated pyramids and deep serrated grooves.
    # End heights agree so adjacent modular lengths meet without a step.
    sections = []
    stations = (-.5, -.36, -.19, -.035, .18, .37, .5)
    for i, along in enumerate(stations):
        h = height*(.78 if i in (0, 6) else 1 if i == 3 else rng.uniform(.82, .97))
        ridge_z = z + (0 if i in (0, 6) else rng.uniform(-.1, .1)*depth)
        px = x+width*along
        front = (px, y+slope*depth/2, z-depth/2)
        back = (px, y-slope*depth/2, z+depth/2)
        sections.append((front, back, (px, front[1]+h*.72, front[2]),
                         (px, y+h-slope*(ridge_z-z), ridge_z),
                         (px, back[1]+h*.72, back[2])))

    def emit(points):
        uv = [(max(0, min(1, a/p.width+.5)), max(0, min(1, c/p.depth+.5)))
              for a, _, c in points]
        if len(points) == 3:
            p.mesh.triangle(*points, uvs=uv, color=SNOW)
        else:
            p.mesh.quad(*points, uv=uv, color=SNOW)

    # Explicit connected winding also handles thin caps with sloped bases.
    for a, b in zip(sections, sections[1:]):
        emit([a[0], b[0], b[1], a[1]])
        emit([a[0], a[2], b[2], b[0]])
        emit([a[1], b[1], b[4], a[4]])
        emit([a[2], a[3], b[3], b[2]])
        emit([a[3], a[4], b[4], b[3]])
    for section, reverse in ((sections[0], False), (sections[-1], True)):
        points = [section[i] for i in (0, 1, 4, 3, 2)]
        if reverse:
            points.reverse()
        for i in range(1, len(points)-1):
            emit([points[0], points[i], points[i+1]])


def build_cap(p, roof=False):
    _snow(p)
    slope = 1.67/2.6 if roof else 0
    height = .125 if roof else p.height
    _strip(p, p.width, height, p.depth, y=slope*p.depth/2, slope=slope)
    p.add_note('closed connected cap; base contacts support, non-solid modular addition')


def build_railing(p, name):
    _canonical(p, name)
    if name == 'porch_railing_straight':
        _strip(p, 1.8, .075, .16, y=1.041)
        _strip(p, 1.8, .035, .135, y=.201, seed=51)
    elif name == 'porch_railing_corner':
        _strip(p, .3, .075, .12, y=1.041)
        start = len(p.mesh.positions)
        _strip(p, .3, .075, .12, y=1.041)
        p.mesh.positions[start:] = [(z, y+.008, -x) for x, y, z in p.mesh.positions[start:]]
    elif name == 'porch_railing_end':
        _strip(p, .3, .085, .16, y=1.041)
    else:
        _strip(p, .16, .085, .16, y=1.041)
    p.add_note('canonical rail/post geometry retained; collider uses original dimensions')


def build_icicles(p, count=1):
    _snow(p)
    p.begin_material(p.material('ice', roughness=.55))
    for i in range(count):
        length = p.height if i == 0 else p.height*(.35 + .5*math.sin(i*2.8)**2)
        radius = p.depth/2*(1 if i == 0 else .62)
        x = 0 if count == 1 else -p.width/2 + p.depth/2 + (p.width-p.depth)*i/(count-1)
        ring = [(x+radius*math.cos(math.tau*j/5), p.height,
                 radius*math.sin(math.tau*j/5)) for j in range(5)]
        tip = (x + radius*.12, p.height-length, radius*.08)
        center = (x, p.height-length*.2, 0)
        for j in range(5):
            _face(p, [ring[j], ring[(j+1)%5], tip], center, ICE)
        for j in range(1, 4):
            _face(p, [ring[0], ring[j], ring[j+1]], center, ICE)
    _fit(p)
    p.add_note('five-sided closed icicles; mount top at underside + 15 mm, tips hang down')


PROPS = {}
for name in SIZES:
    if name.startswith('tree_snow'):
        build = partial(build_tree, heavy=name.endswith('02'))
    elif name in ('boulder_snow', 'rock_face_snow'):
        build = partial(build_rock, cliff=name.startswith('rock'))
    elif name.startswith('drift_') or name == 'mound_irregular':
        build = partial(build_drift, wall=name in ('drift_wall', 'drift_fence'))
    elif name.startswith('railing_snow') or name == 'fence_post_snow':
        base = 'fence_post' if name == 'fence_post_snow' else 'porch_railing_' + name.removeprefix('railing_snow_')
        build = partial(build_railing, name=base)
    elif name.startswith('icicle_'):
        build = partial(build_icicles, count=7 if name.endswith('mixed') else 3 if name.endswith('sparse') else 1)
    else:
        build = partial(build_cap, roof=name == 'snow_roof_slope')
    PROPS['winter:' + name] = build

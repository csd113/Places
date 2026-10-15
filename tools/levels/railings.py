"""Offline assembly of file-backed railing sections and shared upright posts.

The returned level contains ordinary static props. No linking or geometry
construction runs in the player. Points are world (x, y, z) baseline anchors;
the helper converts them to Places' floor-relative prop placement contract.
"""
from __future__ import annotations

import math


def floor_height(level: dict, x: float, z: float, near: float | None = None) -> float:
    """Resolve the first room (prop origin), or the nearest storey support."""
    rooms = [r for r in level['rooms']
             if r['x']-.01 <= x <= r['x']+r['width']+.01
             and r['z']-.01 <= z <= r['z']+r['depth']+.01]
    if not rooms:
        raise ValueError(f'Railing anchor ({x}, {z}) has no supporting room')
    offset = 0
    for region in level.get('floor_regions', []):
        if region['x'] <= x <= region['x']+region['width'] and region['z'] <= z <= region['z']+region['depth']:
            offset = region['offset_y']
    for key in ('stairs', 'ramps'):
        for surface in level.get(key, []):
            if not (surface['x'] <= x <= surface['x']+surface['width']
                    and surface['z'] <= z <= surface['z']+surface['depth']):
                continue
            axis = 'x' if surface['width'] >= surface['depth'] else 'z'
            run = surface['width'] if axis == 'x' else surface['depth']
            fraction = min(1, max(0, ((x if axis == 'x' else z)-surface[axis])/run))
            if key == 'stairs':
                fraction = min(surface['steps'], math.floor(fraction*surface['steps'])+1)/surface['steps']
            offset = surface.get('offset_y', 0)+fraction*surface['rise']
    heights = [room.get('floor_y', 0)+offset for room in rooms]
    return heights[0] if near is None else min(heights, key=lambda y: abs(y-near))


class RailingAuthor:
    """Share support posts across runs and fit bounded, unpitched modules."""

    def __init__(self, level: dict, catalog: dict):
        self.level = level
        self.catalog = catalog
        self.posts: dict[tuple, tuple[dict, float]] = {}

    def run(self, identity: str, points: list[tuple[float, float, float]], *,
            section='outdoor:porch_railing_straight', post='outdoor:porch_post',
            spacing=1.8, materials=None, post_materials=None,
            stock_height=1.05, stock_post_height=1.05, stock_depth=None) -> list[dict]:
        """Link a polyline, using one support at each corner and terminal.

        Horizontal sections compress only their run axis; uprights keep their
        cross-section and height. Rails penetrate support sides by 8 mm, below
        the cap plane. Equal bays avoid a crowded short remainder at the end.
        Supports sample their whole footprint and bury their base by 6 mm.
        """
        if len(points) < 2 or not math.isfinite(spacing) or spacing <= .3:
            raise ValueError('A railing needs at least two points and spacing > 0.3 m')
        if any(not math.isfinite(v) for p in points for v in p):
            raise ValueError('Railing anchors must be finite')
        added = []
        size = self.catalog[section]['size']
        post_size = self.catalog[post]['size']
        for leg, (start, finish) in enumerate(zip(points, points[1:])):
            dx, dy, dz = (b-a for a, b in zip(start, finish))
            length = math.hypot(dx, dz)
            if length < .3 or abs(dy) > length*2:
                raise ValueError('Railing legs must be >= 0.3 m and follow a walkable slope')
            count = math.ceil(length/spacing-1e-7)
            nodes = [tuple(a+(b-a)*i/count for a, b in zip(start, finish)) for i in range(count+1)]
            supports = []
            for i, (x, baseline, z) in enumerate(nodes):
                key = (round(x, 4), round(baseline, 4), round(z, 4), post,
                       tuple(sorted((post_materials or {}).items())))
                if key not in self.posts:
                    half = max(post_size[0], post_size[2])*.55
                    ground = [floor_height(self.level, x+ox, z+oz, baseline)
                              for ox in (-half, 0, half) for oz in (-half, 0, half)]
                    base = min(ground)-.006
                    if max(ground)-baseline > .1 or baseline-min(ground) > .15:
                        raise ValueError(f'{identity}: support at ({x}, {z}) is outside its floor/slope')
                    scale = (baseline+max(stock_height+.012, stock_post_height)-base)/stock_post_height
                    piece = dict(id=f'{identity}_post_{leg}_{i}', model=post,
                                 x=round(x, 5), z=round(z, 5),
                                 y=round(base-floor_height(self.level, x, z), 5),
                                 scale=round(scale, 6),
                                 size=[min(.12, post_size[0]), stock_post_height,
                                       min(.12, post_size[2])], solid=True)
                    if post_materials:
                        piece['material_overrides'] = dict(post_materials)
                    self.level['props'].append(piece)
                    # Cap/foot envelopes may be wider than the shaft. Join the
                    # 12 cm stock body, rather than leaving air under a cap.
                    self.posts[key] = (piece, min(.12, post_size[0], post_size[2])*scale/2-.008)
                supports.append(self.posts[key])
            for i, (a, b) in enumerate(zip(nodes, nodes[1:])):
                left, right = supports[i][1], supports[i+1][1]
                bay_length = length/count
                fractions = (left/bay_length, 1-right/bay_length)
                ends = [tuple(v+(w-v)*f for v, w in zip(a, b)) for f in fractions]
                x, baseline, z = ((v+w)/2 for v, w in zip(*ends))
                piece = dict(id=f'{identity}_section_{leg}_{i}', model=section,
                             x=round(x, 5), z=round(z, 5),
                             y=round(baseline-.006-floor_height(self.level, x, z), 5),
                             rotation_degrees=round(math.degrees(math.atan2(-dz, dx)) % 360, 5),
                             size=[size[0], stock_height,
                                   stock_depth if stock_depth is not None else min(.12, size[2])], solid=True,
                             span=dict(source_length=size[0], length=round(bay_length-left-right, 6),
                                       rise=round(ends[1][1]-ends[0][1], 6)))
                if materials:
                    piece['material_overrides'] = dict(materials)
                self.level['props'].append(piece)
                added.append(piece)
        return added

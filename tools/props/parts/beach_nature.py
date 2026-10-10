"""Beach terrain and vegetation, built deterministically from committed artwork.

All shells are closed and flat shaded by the runtime's geometric normals.
Terrain is decorative: walkable floors, slopes and narrow trunk/arch collision
remain separate. Opt-in placed_components supplies the arch's pier/header
collision. The common Beach atlas has a fixed 4 x 4 layout.
"""
from __future__ import annotations

import math
from pathlib import Path

from geometry import _cross, _dot, _sub, inspect
from mesh import PropBuilder
from parts.refreshed import load_atlas_from

ROOT = Path(__file__).resolve().parents[3] / "assets/environment/beach/props/models"
REGIONS = (
    "sand", "grass", "rock", "stucco", "wood", "plank", "palmtrunk", "palmleaf",
    "blue", "coral", "yellow", "ivory", "dark", "turquoise", "window", "foam",
)
WHITE = (255, 255, 255)
SIZES = {
    "sand_patch": (8.0, 0.22, 6.0),
    "grassy_bank": (8.0, 0.75, 5.0),
    "coastal_rock": (1.2, 1.2, 1.05),
    "coastal_rock_wide": (2.6, 1.5, 2.0),
    "island": (10.0, 3.6, 7.0),
    "sea_arch": (13.0, 5.8, 4.5),
    "palm": (4.8, 6.0, 4.4),
    "palm_small": (3.5, 4.2, 3.2),
    "town_shrub": (1.3, 0.8, 1.15),
    "shoreline_foam": (8.0, 0.04, 1.1),
}


def _atlas(p: PropBuilder):
    p.ao_strength = 0.0
    tex = load_atlas_from(p, ROOT / "beach_palette.png", REGIONS)
    p.begin_material(p.material("beach_stock"))
    return tex


def _triangle(p, a, b, c, uv, color=WHITE):
    u0, v0, u1, v1 = uv
    p.mesh.triangle(a, b, c, [(u0, v1), (u1, v1), ((u0 + u1) / 2, v0)], color)


def _cap(p, ring, uv, color=WHITE):
    center = tuple(sum(point[axis] for point in ring) / len(ring) for axis in range(3))
    low = tuple(min(point[axis] for point in ring) for axis in range(3))
    high = tuple(max(point[axis] for point in ring) for axis in range(3))
    u0, v0, u1, v1 = uv
    def planar(point):
        return (u0 + (point[0] - low[0]) / max(high[0] - low[0], .001) * (u1 - u0),
                v0 + (point[2] - low[2]) / max(high[2] - low[2], .001) * (v1 - v0))
    for i, point in enumerate(ring):
        nxt = ring[(i + 1) % len(ring)]
        p.mesh.triangle(center, point, nxt, [planar(center), planar(point), planar(nxt)], color)


def _shell(p, rings, side_uv, top_uv=None, colors=None):
    """Connected sealed ring shell; winding is checked by _finish."""
    count = len(rings[0])
    if any(len(ring) != count for ring in rings):
        raise ValueError("ring shell requires equal vertex counts")
    for level, (lower, upper) in enumerate(zip(rings, rings[1:])):
        uv = side_uv[level] if isinstance(side_uv, list) else side_uv
        u0, v0, u1, v1 = uv
        group_low, group_high = 0, len(rings) - 1
        if isinstance(side_uv, list):
            group_low, group_high = level, level + 1
            while group_low > 0 and side_uv[group_low - 1] == uv:
                group_low -= 1
            while group_high < len(side_uv) and side_uv[group_high] == uv:
                group_high += 1
        low_v = v1 + (v0 - v1) * (level - group_low) / (group_high - group_low)
        high_v = v1 + (v0 - v1) * (level + 1 - group_low) / (group_high - group_low)
        for i in range(count):
            nxt = (i + 1) % count
            color = colors[i % len(colors)] if colors else WHITE
            coords = [(u0 + (u1 - u0) * i / count, low_v),
                      (u0 + (u1 - u0) * i / count, high_v),
                      (u0 + (u1 - u0) * (i + 1) / count, high_v),
                      (u0 + (u1 - u0) * (i + 1) / count, low_v)]
            p.mesh.quad(lower[i], upper[i], upper[nxt], lower[nxt], uv=coords, color=color)
    _cap(p, rings[0], side_uv[0] if isinstance(side_uv, list) else side_uv)
    _cap(p, rings[-1], top_uv or (side_uv[-1] if isinstance(side_uv, list) else side_uv))


def _finish(p):
    # Keep the established catalogue origin convention, including asymmetric
    # palms. Collision recipes use the measured trunk offset, not a canopy box.
    low, high = p.mesh.bounds()
    span = tuple(high[i] - low[i] for i in range(3))
    center = ((low[0] + high[0]) / 2, low[1], (low[2] + high[2]) / 2)
    p.mesh.positions = [
        tuple(round((point[i] - center[i]) * p.size[i] / span[i], 6) for i in range(3))
        for point in p.mesh.positions
    ]
    inspect(p.mesh.positions, p.mesh.indices, repair=True)
    report = inspect(p.mesh.positions, p.mesh.indices)
    if any(report[field] for field in ("degenerate", "boundary_edges", "nonmanifold_edges",
                                       "inconsistent_edges", "flipped_triangles",
                                       "contradictory_components")):
        raise ValueError(f"{p.id}: invalid nature topology: {report}")
    p.mesh.validate(p.size)


def _ground(p, grass=False):
    tex = _atlas(p)
    # The north (-Z) edge curves gently inland and has unequal facets, matching
    # the calm shore rather than a circular island or a jagged mountain patch.
    outline = [(-4, -2.25), (-3.2, -2.70), (-1.8, -2.96), (0, -3),
               (1.55, -2.83), (2.95, -2.45), (4, -1.85),
               (4, 3), (2.1, 3), (0, 3), (-2.15, 3), (-4, 3)]
    base = [(x, 0, z) for x, z in outline]
    top = []
    for i, (x, z) in enumerate(outline):
        height = (0.26 + (z + 3) / 6 * 0.49) if grass else (0.15 + (z + 3) / 6 * 0.055)
        top.append((x, height, z))
    uv = tex.uv("grass" if grass else "sand", inset=2)
    side = tex.uv("sand", inset=2)
    for i in range(len(outline)):
        nxt = (i + 1) % len(outline)
        p.mesh.quad(base[i], top[i], top[nxt], base[nxt], uv=side, color=WHITE)
    _cap(p, base, side)
    center = (0, 0.75 if grass else 0.22, 0.25)
    # Fitted continuous planar UVs keep the big clean facets readable.
    u0, v0, u1, v1 = uv
    def planar(point):
        return (u0 + (point[0] + 4) / 8 * (u1 - u0),
                v0 + (point[2] + 3) / 6 * (v1 - v0))
    for i in range(len(top)):
        a, b = top[i], top[(i + 1) % len(top)]
        p.mesh.triangle(center, b, a, [planar(center), planar(b), planar(a)], WHITE)
    _finish(p)
    p.add_note("curved -Z coastal edge; visual only, author real floor/region/ramp beneath the surface")


def sand_patch(p):
    _ground(p)


def grassy_bank(p):
    _ground(p, grass=True)


def _rock(p, wide=False):
    tex = _atlas(p)
    angles = ((0, .52, 1.19, 1.95, 2.65, 3.33, 4.10, 4.87, 5.72) if wide else
              (0, .62, 1.32, 2.01, 2.77, 3.49, 4.22, 4.93, 5.70))
    jitter = ((1, 1.03, .83, .96, .88, 1.06, .94, .87, 1.02) if wide else
              (1, .88, 1.04, .94, 1.03, .86, .97, 1.05, .91))
    # Unequal rings produce broad hewn planes and an off-centre broken crown.
    rings = []
    for level, radius, y in ((0, 1, 0), (1, .91, .52), (2, .58, 1), (3, .25, 1.2)):
        ring = []
        for i, angle in enumerate(angles):
            x = math.cos(angle) * radius * jitter[i]
            z = math.sin(angle) * radius * jitter[(i + 2) % len(jitter)]
            lean = (.07 if wide else -.09) * level
            ring.append((x + lean, y + (.04 * math.sin(i * 1.7) if level in (1, 2) else 0),
                         z + .05 * level))
        rings.append(ring)
    _shell(p, rings, tex.uv("rock", inset=2), colors=((247, 247, 255), (229, 239, 255),
                                                     (255, 244, 249), (242, 246, 255)))
    _finish(p)
    p.add_note("closed irregular blue-violet coastal stone, nine unequal facets and a broken crown")
    if wide:
        p.add_note("wide low shore variant, not an enlarged copy of the tall rock")


def coastal_rock(p):
    _rock(p)


def coastal_rock_wide(p):
    _rock(p, wide=True)


def island(p):
    tex = _atlas(p)
    ring_count = 11
    base = []
    middle = []
    rock_top = []
    grass_top = []
    for i in range(ring_count):
        angle = math.tau * i / ring_count
        irregular = 1 + .12 * math.sin(i * 2.7)
        x, z = 5 * math.cos(angle) * irregular, 3.5 * math.sin(angle) * irregular
        base.append((x, 0, z))
        middle.append((x * .83 + .23, 1.7 + .22 * math.sin(i * 2.1), z * .88))
        rock_top.append((x * .64, 3.32 + .12 * math.sin(i * 1.9), z * .65))
        grass_top.append((x * .63, 3.58 + .02 * math.sin(i * 2.3), z * .64))
    rock, grass = tex.uv("rock", inset=2), tex.uv("grass", inset=2)
    _shell(p, [base, middle, rock_top, grass_top], [rock, rock, grass], top_uv=grass,
           colors=((239, 245, 255), (255, 246, 252), (240, 241, 254)))
    _finish(p)
    p.add_note("angular offshore cliff with connected thin green cap, closed below the waterline")
    p.add_note("visual scenery; separate supported collision if made explorable")


def _sea_arch_profiles():
    """Immutable source profiles shared by the visual and collision recipe."""
    outer = [(-6.5, 0), (-5.95, 2.8), (-5.25, 4.80), (-3.4, 5.65),
             (-.4, 5.8), (3.6, 5.55), (5.2, 4.65), (6.5, 0)]
    inner = [(-3, 0), (-2.65, 2.1), (-2.25, 3.2), (-1.5, 3.8),
             (0, 3.85), (1.9, 3.75), (2.7, 2.6), (3.45, 0)]
    front_outer = [(x, y, 2.25 - .18 * math.sin(i * 1.7)) for i, (x, y) in enumerate(outer)]
    back_outer = [(x + .13 * math.sin(i), y - .06 * math.sin(i * 1.2), -2.25)
                  for i, (x, y) in enumerate(outer)]
    front_inner = [(x, y, 1.92) for x, y in inner]
    back_inner = [(x + .18, y + .08 * math.sin(i), -1.91) for i, (x, y) in enumerate(inner)]
    return front_outer, back_outer, front_inner, back_inner


def sea_arch(p):
    tex = _atlas(p)
    rock, grass = tex.uv("rock", inset=2), tex.uv("grass", inset=2)
    # Paired paths form the full continuous arch section; the passage is real
    # empty space, open through the front/back and down to the sea.
    front_outer, back_outer, front_inner, back_inner = _sea_arch_profiles()
    count = len(front_outer)
    for i in range(count - 1):
        nxt = i + 1
        color = (245, 246, 255) if i % 2 else (231, 240, 255)
        u0, v0, u1, v1 = rock
        def facade(point):
            return (u0 + (point[0] + 6.5) / 13.0 * (u1 - u0),
                    v1 - point[1] / 5.8 * (v1 - v0))
        front = [front_outer[i], front_inner[i], front_inner[nxt], front_outer[nxt]]
        back = [back_outer[nxt], back_inner[nxt], back_inner[i], back_outer[i]]
        p.mesh.quad(*front, uv=[facade(point) for point in front], color=color)
        p.mesh.quad(*back, uv=[facade(point) for point in back], color=WHITE)
        p.mesh.quad(front_outer[i], front_outer[nxt], back_outer[nxt], back_outer[i],
                    uv=grass if i in (2, 3, 4, 5) else rock, color=WHITE)
        p.mesh.quad(front_inner[nxt], front_inner[i], back_inner[i], back_inner[nxt],
                    uv=rock, color=(245, 248, 255))
    for i in (0, count - 1):
        p.mesh.quad(front_outer[i], back_outer[i], back_inner[i], front_inner[i],
                    uv=rock, color=WHITE)
    # A thin connected grass ribbon crowns the roof. It has real edge depth,
    # seated 15 mm into the rock, rather than disconnected painted green cards.
    def offset(point, height):
        return (point[0], point[1] + height, point[2])
    for i in range(2, 6):
        surface = [front_outer[i], front_outer[i + 1], back_outer[i + 1], back_outer[i]]
        lower = [offset(point, -.015) for point in surface]
        upper = [offset(point, .12) for point in surface]
        p.mesh.quad(*lower, uv=grass, color=WHITE)
        p.mesh.quad(*upper, uv=grass, color=WHITE)
        p.mesh.quad(lower[0], lower[1], upper[1], upper[0], uv=grass, color=WHITE)
        p.mesh.quad(lower[2], lower[3], upper[3], upper[2], uv=grass, color=WHITE)
        if i == 2:
            p.mesh.quad(lower[3], lower[0], upper[0], upper[3], uv=grass, color=WHITE)
        if i == 5:
            p.mesh.quad(lower[1], lower[2], upper[2], upper[1], uv=grass, color=WHITE)
    _finish(p)
    p.add_note("distinctive asymmetrical flat-crowned sea arch; real faceted opening and thin grass crown")
    p.add_note("opening local x approximately -3..3.4, y 0..3.8; placed_components('sea_arch', ...) supplies separate pier/header collision")


def _arch_local_contours():
    """Apply the existing export's exact origin/size fit without reading art."""
    front_outer, back_outer, front_inner, back_inner = _sea_arch_profiles()
    all_points = front_outer + back_outer + front_inner + back_inner
    all_points += [(x, y + offset, z) for ring in (front_outer, back_outer)
                   for x, y, z in ring[2:7] for offset in (-.015, .12)]
    low = [min(point[axis] for point in all_points) for axis in range(3)]
    high = [max(point[axis] for point in all_points) for axis in range(3)]
    center = ((low[0] + high[0]) / 2, low[1], (low[2] + high[2]) / 2)
    scale = [SIZES["sea_arch"][axis] / (high[axis] - low[axis]) for axis in range(3)]

    def local(point):
        return tuple(round((point[axis] - center[axis]) * scale[axis], 6) for axis in range(3))

    front = [local(point) for point in front_outer + list(reversed(front_inner))]
    back = [local(point) for point in back_outer + list(reversed(back_inner))]
    front_z = min(local(point)[2] for point in front_inner) - .006
    back_z = max(local(point)[2] for point in back_inner) + .006
    return front, back, back_z, front_z


def _section_intervals(contour, height):
    """Inside spans of the open-bottom rock section at one local height."""
    crossings = []
    for a, b in zip(contour, contour[1:] + contour[:1]):
        if min(a[1], b[1]) <= height < max(a[1], b[1]):
            crossings.append(a[0] + (b[0] - a[0]) * (height - a[1]) / (b[1] - a[1]))
    crossings.sort()
    return list(zip(crossings[::2], crossings[1::2]))


def _intersect_intervals(first, second):
    return [(max(a, c), min(b, d)) for a, b in first for c, d in second
            if min(b, d) > max(a, c)]


def _arch_stock_triangles(front, back):
    """The exported rock's exact triangle split, after its existing size fit."""
    front_outer, back_outer = front[:8], back[:8]
    front_inner, back_inner = list(reversed(front[8:])), list(reversed(back[8:]))
    triangles = []

    def quad(a, b, c, d):
        triangles.extend(((a, b, c), (a, c, d)))

    for i in range(7):
        nxt = i + 1
        quad(front_outer[i], front_inner[i], front_inner[nxt], front_outer[nxt])
        quad(back_outer[nxt], back_inner[nxt], back_inner[i], back_outer[i])
        quad(front_outer[i], front_outer[nxt], back_outer[nxt], back_outer[i])
        quad(front_inner[nxt], front_inner[i], back_inner[i], back_inner[nxt])
    for i in (0, 7):
        quad(front_outer[i], back_outer[i], back_inner[i], front_inner[i])
    return triangles


def _inside_stock(point, triangles):
    direction = (1.0, .271, .413)
    distances = []
    for a, b, c in triangles:
        ab, ac = _sub(b, a), _sub(c, a)
        cross = _cross(direction, ac)
        determinant = _dot(ab, cross)
        if abs(determinant) < 1e-10:
            continue
        origin = _sub(point, a)
        u = _dot(origin, cross) / determinant
        other = _cross(origin, ab)
        v = _dot(direction, other) / determinant
        distance = _dot(ac, other) / determinant
        if u >= -1e-8 and v >= -1e-8 and u + v <= 1 + 1e-8 and distance > 1e-7:
            distances.append(round(distance, 6))
    return len(set(distances)) % 2 == 1


def _triangle_touches_box(triangle, center, half):
    """Exact separating-axis test for a triangle against an axis-aligned box."""
    points = [_sub(point, center) for point in triangle]
    edges = [_sub(points[(i + 1) % 3], points[i]) for i in range(3)]
    axes = [(1, 0, 0), (0, 1, 0), (0, 0, 1)]
    axes += [_cross(edge, axis) for edge in edges for axis in axes[:3]]
    axes.append(_cross(edges[0], edges[1]))
    for axis in axes:
        if _dot(axis, axis) < 1e-20:
            continue
        projection = [_dot(point, axis) for point in points]
        radius = sum(abs(axis[i]) * half[i] for i in range(3))
        if min(projection) > radius or max(projection) < -radius:
            return False
    return True


def _inset_stock_box(box, triangles):
    """Keep a carrier wholly inside real triangulated rock, not its end planes.

    A connected box with an inside centre and no boundary triangle crossing
    its interior is contained in this closed stock. Small problematic foot or
    crest bands are omitted when their centre itself falls outside the rock.
    """
    x, base, z, width, height, depth = box
    center = (x, base + height / 2, z)
    if not _inside_stock(center, triangles):
        return None
    half = [width / 2, height / 2, depth / 2]
    for _ in range(100):
        if not any(_triangle_touches_box(triangle, center, half) for triangle in triangles):
            return [x, center[1] - half[1], z, half[0] * 2, half[1] * 2, half[2] * 2]
        half = [extent * .96 for extent in half]
    return None


def structural_components(name):
    """Tight, separate sea-arch pier/header carriers in metre-local space.

    Every box is inset into the exact triangulated rock stock, including its
    nonplanar roof/foot facets. Their union preserves the real open passage.
    The hidden carrier renders
    only a 3 mm cube; its authored size supplies collision independently.
    Caller supplies a supported approach/floor beneath the visual arch.
    """
    if name.startswith("beach:"):
        name = name.split(":", 1)[1]
    if name != "sea_arch":
        raise ValueError(f"no separate Beach nature collision recipe for {name}")
    front, back, back_z, front_z = _arch_local_contours()
    triangles = _arch_stock_triangles(front, back)
    levels = sorted({point[1] for contour in (front, back) for point in contour})
    boxes = []
    for base, top in zip(levels, levels[1:]):
        if top - base <= .006:
            continue
        # Profile breakpoints partition every linear edge. Intersecting the
        # two end sections and both height limits keeps boxes out of the void,
        # including the slightly offset rear opening and asymmetric piers.
        inside = [(-SIZES["sea_arch"][0] / 2, SIZES["sea_arch"][0] / 2)]
        for height in (base + 1e-7, (base + top) / 2, top - 1e-7):
            for contour in (front, back):
                inside = _intersect_intervals(inside, _section_intervals(contour, height))
        for left, right in inside:
            if right - left > .012:
                candidate = [(left + right) / 2, base + .003, (back_z + front_z) / 2,
                             right - left - .006, top - base - .006, front_z - back_z]
                fitted = _inset_stock_box(candidate, triangles)
                if fitted is not None:
                    boxes.append(fitted)
    return {"collision_boxes": boxes}


def placed_components(name, x, z, *, base_y=0, floor_y=0, rotation_degrees=0, identity=None,
                      floor_at=None):
    """Return usable v3 placement arrays for a visual arch and its collision.

    ``base_y`` is absolute world Y; prop ``y`` is compensated against the
    existing supported floor queried at each world position. ``floor_y`` is
    the fallback floor and ``floor_at(x,z)`` may resolve existing regions and
    ramps. Quarter-turn rotations match beach_structures.placed_components.
    No new floor is invented; caller owns the floor and instance namespace.
    """
    if name.startswith("beach:"):
        name = name.split(":", 1)[1]
    local = structural_components(name)
    if not all(math.isfinite(value) for value in (x, z, base_y, floor_y, rotation_degrees)):
        raise ValueError("Beach placement coordinates must be finite")
    if floor_at is not None and not callable(floor_at):
        raise ValueError("floor_at must be a callable world-floor query")
    yaw = rotation_degrees % 360
    quarter = round(yaw / 90) % 4
    if abs((yaw - quarter * 90 + 180) % 360 - 180) > 1e-6:
        raise ValueError("Beach structural placement supports quarter-turn rotations")
    identity = identity or "beach_" + name

    def point(px, pz):
        return ((x + px, z + pz), (x + pz, z - px),
                (x - px, z - pz), (x - pz, z + px))[quarter]

    def supporting_floor(px, pz):
        height = floor_y if floor_at is None else floor_at(px, pz)
        if height is None or not math.isfinite(height):
            raise ValueError(f"Beach placement has no finite supporting floor at {px},{pz}")
        return height

    result = {"props": [], "floor_regions": [], "stairs": [], "archways": []}
    result["props"].append(dict(id=identity, model="beach:" + name, x=x, z=z,
                                y=base_y - supporting_floor(x, z), rotation_degrees=yaw,
                                size=list(SIZES[name]), solid=False))
    proxy_scale = .05
    for i, (px, py, pz, width, height, depth) in enumerate(local["collision_boxes"]):
        wx, wz = point(px, pz)
        result["props"].append(dict(id=f"{identity}_solid_{i}", model="outdoor:collision_peg",
                                    x=wx, z=wz, y=base_y + py - supporting_floor(wx, wz),
                                    rotation_degrees=yaw, scale=proxy_scale,
                                    size=[width / proxy_scale, height / proxy_scale, depth / proxy_scale],
                                    solid=True, occludes=False))
    return result


def _frond(p, root, angle, length, droop, rise, uv, tint):
    direction = (math.cos(angle), math.sin(angle))
    lateral = (-direction[1], direction[0])
    stations = (0, .16, .32, .50, .68, .85)
    widths = (.065, .29, .23, .36, .23, .17)
    rings = []
    for t, width in zip(stations, widths):
        width *= 1.55 * length / 2.3
        x, z = root[0] + direction[0] * length * t, root[2] + direction[1] * length * t
        y = root[1] + rise * math.sin(math.pi * t) - droop * t * t
        fold = .11 * math.sin(math.pi * t) + .025
        # Five-sided folded leaf section: ridge above two broad planes, a
        # thin flat underside and real thickness along the serrated edges.
        rings.append([(x + lateral[0] * width, y, z + lateral[1] * width),
                      (x, y + fold, z),
                      (x - lateral[0] * width, y, z - lateral[1] * width),
                      (x - lateral[0] * width, y - .018, z - lateral[1] * width),
                      (x + lateral[0] * width, y - .018, z + lateral[1] * width)])
    # One fitted repeat per complete frond; v follows the curved leaf's length.
    u0, v0, u1, v1 = uv
    cross_u = (u0, (u0 + u1) / 2, u1, u1, u0)
    for index, (first, second) in enumerate(zip(rings, rings[1:])):
        v_first, v_second = v1 + (v0 - v1) * stations[index], v1 + (v0 - v1) * stations[index + 1]
        for edge in range(5):
            nxt = (edge + 1) % 5
            coords = [(cross_u[edge], v_first), (cross_u[nxt], v_first),
                      (cross_u[nxt], v_second), (cross_u[edge], v_second)]
            p.mesh.quad(first[edge], first[nxt], second[nxt], second[edge], uv=coords, color=tint)
    first = rings[0]
    _triangle(p, first[0], first[1], first[2], uv, tint)
    p.mesh.quad(first[0], first[2], first[3], first[4], uv=uv, color=tint)
    last = rings[-1]
    tip = (root[0] + direction[0] * length, root[1] - droop,
           root[2] + direction[1] * length)
    bottom = (tip[0], tip[1] - .018, tip[2])
    tip_uv = ((u0 + u1) / 2, v0)
    last_v = v1 + (v0 - v1) * stations[-1]
    p.mesh.triangle(last[0], last[1], tip, [(u0, last_v), ((u0 + u1) / 2, last_v), tip_uv], tint)
    p.mesh.triangle(last[1], last[2], tip, [((u0 + u1) / 2, last_v), (u1, last_v), tip_uv], tint)
    p.mesh.triangle(last[3], last[4], bottom, [(u1, last_v), (u0, last_v), tip_uv], tint)
    p.mesh.quad(last[2], last[3], bottom, tip,
                uv=[(u1, last_v), (u1, last_v), tip_uv, tip_uv], color=tint)
    p.mesh.quad(last[4], last[0], tip, bottom,
                uv=[(u0, last_v), (u0, last_v), tip_uv, tip_uv], color=tint)


def _palm(p, small=False):
    tex = _atlas(p)
    trunk, leaf = tex.uv("palmtrunk", inset=2), tex.uv("palmleaf", inset=2)
    rings = []
    bands = 13
    for i in range(bands):
        t = i / (bands - 1)
        center = (1.00 * t * t - .15 * math.sin(math.pi * t), 4.8 * t, .22 * t * t)
        radius = .30 * (1 - .43 * t) * (1 + (.018 if i % 2 else -.012))
        rings.append([(center[0] + radius * math.cos(math.tau * n / 7), center[1],
                       center[2] + radius * math.sin(math.tau * n / 7)) for n in range(7)])
    # One trunk-sheet repeat from base to crown preserves the broad painted
    # bands; alternating ring radius makes them readable in silhouette too.
    u0, v0, u1, v1 = trunk
    for i in range(bands - 1):
        for n in range(7):
            nxt = (n + 1) % 7
            coords = [(u0 + (u1 - u0) * n / 7, v1 + (v0 - v1) * i / 12),
                      (u0 + (u1 - u0) * (n + 1) / 7, v1 + (v0 - v1) * i / 12),
                      (u0 + (u1 - u0) * (n + 1) / 7, v1 + (v0 - v1) * (i + 1) / 12),
                      (u0 + (u1 - u0) * n / 7, v1 + (v0 - v1) * (i + 1) / 12)]
            p.mesh.quad(rings[i][n], rings[i][nxt], rings[i + 1][nxt], rings[i + 1][n],
                        uv=coords, color=WHITE)
    _cap(p, rings[0], trunk)
    _cap(p, rings[-1], trunk)
    for n in range(8):
        angle = math.tau * n / 8 + (.13 if small else .04)
        length = 2.05 + .27 * math.sin(n * 2.3 + (.9 if small else 0))
        droop = (.32 if n in (1, 4) else .8 + .44 * (.5 + .5 * math.sin(n * 1.8)))
        rise = .55 + .25 * math.cos(n * 2.1)
        tint = ((255, 255, 255), (238, 255, 223), (251, 251, 237), (230, 247, 224))[n % 4]
        _frond(p, (1, 4.79 + .055 * (n % 3), .22), angle, length, droop, rise, leaf, tint)
    for angle in (.72 * math.pi, 1.80 * math.pi):
        _frond(p, (1, 4.91, .22), angle + (.13 if small else 0), 1.1, -.50, .45,
               leaf, (246, 255, 229))
    _finish(p)
    p.add_note("bent seven-sided tapered ochre trunk with thirteen broad bands; eight uneven folded pointed drooping fronds and two rising crown shoots")
    p.add_note("static opaque leaves retain real silhouettes and baked cast shadows; solid:false, separate narrow trunk collider")


# Measured exported local trunk bounds, separate from the much wider crown.
# These are authoring metadata, not an implicit runtime collision component.
PALM_TRUNKS = {
    "palm": {"base_center": (-1.120002, 0, .044545),
             "min": (-1.463155, 0, -.266841), "max": (.229805, 5.107267, .459102)},
    "palm_small": {"base_center": (-1.012678, 0, -.009627),
                   "min": (-1.265884, 0, -.235002), "max": (-.016684, 3.575087, .290421)},
}


def palm(p):
    _palm(p)


def palm_small(p):
    _palm(p, small=True)


def town_shrub(p):
    tex = _atlas(p)
    leaf = tex.uv("grass", inset=2)
    for center, radius, height, phase in [((-.31, 0, .03), .36, .58, .0),
                                        ((.24, 0, -.13), .39, .77, .4),
                                        ((.17, 0, .25), .31, .52, .8)]:
        rings = []
        for scale, y in ((.55, 0), (1, height * .39), (.71, height * .78), (.20, height)):
            rings.append([(center[0] + radius * scale * math.cos(math.tau * n / 7 + phase),
                           y, center[2] + radius * scale * math.sin(math.tau * n / 7 + phase))
                          for n in range(7)])
        _shell(p, rings, leaf, colors=((255, 255, 255), (244, 255, 229), (230, 248, 222)))
    _finish(p)
    p.add_note("three unequal faceted green shrub lobes for the town courtyards; no alpha cards")


def shoreline_foam(p):
    tex = _atlas(p)
    uv = tex.uv("foam", inset=2)
    count = 17
    left, right = [], []
    for i in range(count):
        x = -4 + 8 * i / (count - 1)
        curve = .26 * math.sin(i * .23) + .10 * math.sin(i * 1.3)
        width = .18 + .095 * (.5 + .5 * math.sin(i * 2.1))
        left.append((x, 0, curve - width))
        right.append((x, 0, curve + width))
    contour = left + list(reversed(right))
    # A sealed thin scalloped ribbon. The broad irregular white edge is drawn
    # by geometry, not a runtime-generated texture or a rectangular alpha card.
    top = [(x, .04, z) for x, _, z in contour]
    for i in range(len(contour)):
        nxt = (i + 1) % len(contour)
        p.mesh.quad(contour[i], top[i], top[nxt], contour[nxt], uv=uv, color=WHITE)
    for i in range(count - 1):
        p.mesh.quad(left[i], left[i + 1], right[i + 1], right[i], uv=uv, color=WHITE)
        p.mesh.quad(top[i], top[2 * count - 1 - i], top[2 * count - 2 - i], top[i + 1],
                    uv=uv, color=WHITE)
    _finish(p)
    p.add_note("irregular curved opaque ivory foam ribbon, 4 cm sealed depth; position slightly above shoreline")


PROPS = {"beach:" + name: build for name, build in {
    "sand_patch": sand_patch, "grassy_bank": grassy_bank,
    "coastal_rock": coastal_rock, "coastal_rock_wide": coastal_rock_wide,
    "island": island, "sea_arch": sea_arch, "palm": palm, "palm_small": palm_small,
    "town_shrub": town_shrub, "shoreline_foam": shoreline_foam,
}.items()}

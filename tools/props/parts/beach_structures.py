"""Beach concept structures: closed shells, fitted committed atlas, metres.

Only the immutable Beach board's structures are reconstructed here. The
three town colours, real door arches, deck, stair, terrace and parapet are
meaningful authoring modules. Hollow structures are decorative GLBs: their
separate local collision/support recipes below keep openings usable.
"""
from __future__ import annotations

import math
from pathlib import Path

from geometry import inspect
from parts.refreshed import load_atlas_from, orient_outward, solid_box, solid_cylinder

ROOT = Path(__file__).resolve().parents[3] / "assets/environment/beach/props/models"
WHITE = (255, 255, 255)
REGIONS = (
    "sand", "grass", "rock", "stucco", "wood", "plank", "palmtrunk", "palmleaf",
    "blue", "coral", "yellow", "ivory", "dark", "turquoise", "window", "foam",
)
SIZES = {
    "lighthouse": (2.72, 7.70, 2.72),
    "crate": (0.82, 0.82, 0.82),
    "dock": (4.40, 1.60, 2.20),
    "kiosk": (3.20, 3.64, 2.90),
    "stilt_house_blue": (5.40, 6.08, 4.70),
    "hut_brown": (3.50, 3.55, 3.24),
    "umbrella": (2.40, 2.55, 2.40),
    "lounge_chair": (0.74, 1.10, 1.51),
    "signpost": (1.20, 1.76, 0.20),
    "town_house_cream": (4.60, 5.40, 3.90),
    "town_house_blue": (4.20, 5.45, 3.75),
    "town_house_coral": (3.80, 4.00, 3.60),
    "town_arch": (3.60, 2.80, 0.42),
    "town_stairs": (1.40, 2.40, 4.20),
    "town_terrace": (4.00, 0.90, 3.00),
    "town_terrace_base": (4.00, 2.22, 3.00),
    "town_parapet": (2.40, 0.74, 0.25),
    "bunting": (4.40, 0.45, 0.024),
}


def atlas(p):
    p.ao_strength = 0
    texture = load_atlas_from(p, ROOT / "beach_palette.png", REGIONS)
    p.begin_material(p.material("beach_matte"))
    return texture


def box(p, centre, size, uv):
    solid_box(p, centre, size, uv=uv, color=WHITE, shade=False)


def uv_triangle(uv):
    u0, v0, u1, v1 = uv
    return [(u0, v1), (u1, v1), ((u0 + u1) / 2, v0)]


def triangle(p, a, b, c, uv):
    p.mesh.triangle(a, b, c, uvs=uv_triangle(uv), color=WHITE)


def loft(p, rings, uv):
    """Closed convex ring loft; independent hard face UVs and outward winding."""
    start = len(p.mesh.indices)
    n = len(rings[0])
    if any(len(ring) != n for ring in rings):
        raise ValueError("loft rings need matching vertex counts")
    for low, high in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            p.mesh.quad(low[i], low[j], high[j], high[i], uv=uv, color=WHITE)
    for ring in (rings[0], rings[-1]):
        centre = tuple(sum(q[k] for q in ring) / n for k in range(3))
        for i in range(n):
            triangle(p, centre, ring[i], ring[(i + 1) % n], uv)
    centre = tuple(sum(q[k] for ring in rings for q in ring) / (n * len(rings)) for k in range(3))
    orient_outward(p.mesh, start, centre)


def beam(p, a, b, width, depth, uv):
    """Square timber with section perpendicular to its arbitrary centreline."""
    direction = [b[k] - a[k] for k in range(3)]
    length = math.sqrt(sum(v * v for v in direction))
    if length <= 1e-8:
        raise ValueError("beam endpoints must differ")
    axis = [v / length for v in direction]
    ref = (1, 0, 0) if abs(axis[1]) > 0.95 else (0, 1, 0)
    cross = lambda u, v: (u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0])
    right = cross(axis, ref)
    norm = math.sqrt(sum(v*v for v in right))
    right = [v / norm for v in right]
    up = cross(axis, right)
    rings = [[tuple(c[k] + x*width*right[k]/2 + y*depth*up[k]/2 for k in range(3))
              for x, y in ((-1, -1), (1, -1), (1, 1), (-1, 1))] for c in (a, b)]
    loft(p, rings, uv)


def _ear_triangles(polygon):
    """Triangulate a simple 2D contour, including the stair and open arch."""
    area = sum(a[0]*b[1]-b[0]*a[1] for a, b in zip(polygon, polygon[1:] + polygon[:1]))
    order = list(range(len(polygon)))
    if area < 0:
        order.reverse()

    def cross(a, b, c):
        return (b[0]-a[0])*(c[1]-a[1]) - (b[1]-a[1])*(c[0]-a[0])

    result = []
    while len(order) > 3:
        for i, middle in enumerate(order):
            before, after = order[i-1], order[(i+1) % len(order)]
            a, b, c = polygon[before], polygon[middle], polygon[after]
            if cross(a, b, c) <= 1e-10:
                continue
            inside = any(cross(a, b, polygon[j]) >= -1e-10 and
                         cross(b, c, polygon[j]) >= -1e-10 and
                         cross(c, a, polygon[j]) >= -1e-10
                         for j in order if j not in (before, middle, after))
            if inside:
                continue
            result.append((before, middle, after))
            del order[i]
            break
        else:
            raise ValueError("non-simple/degenerate extrusion contour")
    result.append(tuple(order))
    return result


def extrude(p, polygon, low, high, uv, *, axis="z", side_uv=None):
    """Closed extrusion of a simple concave contour; never fills its aperture."""
    start = len(p.mesh.indices)
    side_uv = uv if side_uv is None else side_uv
    if axis == "z":
        points = [[(x, y, z) for x, y in polygon] for z in (low, high)]
    elif axis == "x":
        points = [[(x, y, z) for z, y in polygon] for x in (low, high)]
    else:
        raise ValueError("extrude supports z and x axes")
    for ring in points:
        for a, b, c in _ear_triangles(polygon):
            triangle(p, ring[a], ring[b], ring[c], uv)
    for i in range(len(polygon)):
        j = (i + 1) % len(polygon)
        p.mesh.quad(points[0][i], points[0][j], points[1][j], points[1][i], uv=side_uv, color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices


def hip_roof(p, width, depth, base, rise, uv, thickness=0.12):
    """Sealed overhanging four-slope roof, with a ridge or pointed hip apex."""
    start = len(p.mesh.indices)
    corners = [(-width/2, base+thickness, -depth/2), (width/2, base+thickness, -depth/2),
               (width/2, base+thickness, depth/2), (-width/2, base+thickness, depth/2)]
    lower = [(x, base, z) for x, _, z in corners]
    ridge = max(0.0, (width-depth) / 2)
    left, right = (-ridge, base+rise, 0), (ridge, base+rise, 0)
    if ridge < 1e-5:
        for i in range(4):
            triangle(p, corners[i], corners[(i+1) % 4], left, uv)
    else:
        p.mesh.quad(corners[0], corners[1], right, left, uv=uv, color=WHITE)
        triangle(p, corners[1], corners[2], right, uv)
        p.mesh.quad(corners[2], corners[3], left, right, uv=uv, color=WHITE)
        triangle(p, corners[3], corners[0], left, uv)
    p.mesh.quad(*lower, uv=uv, color=WHITE)
    for i in range(4):
        j = (i + 1) % 4
        p.mesh.quad(lower[i], lower[j], corners[j], corners[i], uv=uv, color=WHITE)
    orient_outward(p.mesh, start, (0, base+rise*.3, 0))


def arch_wall(p, width, height, opening, crown, rise, depth, uv, *, z=0, reveal=None):
    radius = opening / 2
    spring = crown - rise
    contour = [(-width/2, 0), (-width/2, height), (width/2, height),
               (width/2, 0), (radius, 0), (radius, spring)]
    # Eight deliberately visible arch facets, a doorway rather than a dark panel.
    contour += [(radius*math.cos(math.pi*i/8), spring+rise*math.sin(math.pi*i/8)) for i in range(1, 9)]
    contour += [(-radius, 0)]
    extrude(p, contour, z-depth/2, z+depth/2, uv, side_uv=reveal)


def window(p, x, y, z, width, height, texture, *, rotation=0):
    """Closed pane behind one continuous timber ring; +Z-facing by default."""
    wood, pane = texture.uv("wood", inset=2), texture.uv("window", inset=2)
    start_vertex = len(p.mesh.positions)
    box(p, (0, y, 0), (width-.085, height-.085, .018), pane)
    start = len(p.mesh.indices)
    for depth in (-.005, .055):
        outer = [(-width/2, y-height/2, depth), (width/2, y-height/2, depth),
                 (width/2, y+height/2, depth), (-width/2, y+height/2, depth)]
        inner = [(-width/2+.055, y-height/2+.055, depth), (width/2-.055, y-height/2+.055, depth),
                 (width/2-.055, y+height/2-.055, depth), (-width/2+.055, y+height/2-.055, depth)]
        for i in range(4):
            j = (i + 1) % 4
            p.mesh.quad(outer[i], outer[j], inner[j], inner[i], uv=wood, color=WHITE)
    for half in (0, .055):
        extent_x, extent_y = width/2-half, height/2-half
        ring = [(-extent_x, y-extent_y), (extent_x, y-extent_y),
                (extent_x, y+extent_y), (-extent_x, y+extent_y)]
        for i in range(4):
            j = (i + 1) % 4
            p.mesh.quad((*ring[i], -.005), (*ring[j], -.005), (*ring[j], .055), (*ring[i], .055), uv=wood, color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices
    angle = math.radians(rotation)
    for i in range(start_vertex, len(p.mesh.positions)):
        px, py, pz = p.mesh.positions[i]
        p.mesh.positions[i] = (x+px*math.cos(angle)+pz*math.sin(angle), py,
                               z-px*math.sin(angle)+pz*math.cos(angle))


def finish(p, note):
    p.mesh.normalize_origin()
    if p.mesh.degenerate_triangles():
        raise ValueError(f"{p.id}: zero-area triangles")
    p.add_note(note)


def lighthouse(p):
    t = atlas(p)
    ivory, rock, red, dark = (t.uv(n, inset=2) for n in ("ivory", "rock", "coral", "dark"))
    solid_cylinder(p, (0, 0, 0), 1.05, .28, segments=8, uv=rock, color=WHITE, shades=False)
    rings = [[(r*math.cos(math.tau*i/8+math.pi/8), y, r*math.sin(math.tau*i/8+math.pi/8))
              for i in range(8)] for y, r in ((.27, .94), (4.98, .63))]
    loft(p, rings, ivory)
    # Insets follow the actual tapered front plane, avoiding floating stickers.
    def panel(x, y, width, height, uv):
        rings = []
        for offset in (-.005, .008):
            ring = []
            for dx, dy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                py = y+dy*height/2
                radius = .94-(py-.27)*(.31/4.71)
                ring.append((x+dx*width/2, py, radius*math.cos(math.pi/8)+offset))
            rings.append(ring)
        loft(p, rings, uv)
    panel(0, .70, .32, .88, red)
    for y, x, region in ((2.27, -.27, "coral"), (3.45, .25, "dark")):
        panel(x, y, .19, .40, t.uv(region, inset=2))
    solid_cylinder(p, (0, 4.91, 0), 1.36, .15, segments=12, uv=ivory, color=WHITE, shades=False)
    for i in range(12):
        a, b = math.tau*i/12, math.tau*(i+1)/12
        x, z = math.cos(a)*1.24, math.sin(a)*1.24
        box(p, (x, 5.37, z), (.055, .65, .055), ivory)
        beam(p, (x, 5.65, z), (math.cos(b)*1.24, 5.65, math.sin(b)*1.24), .052, .052, ivory)
    solid_cylinder(p, (0, 5.06, 0), .64, 1.33, segments=8, uv=t.uv("window", inset=2), color=WHITE, shades=False)
    for i in range(8):
        a = math.tau*i/8
        box(p, (math.cos(a)*.646, 5.73, math.sin(a)*.646), (.068, 1.32, .068), ivory)
    solid_cylinder(p, (0, 6.35, 0), .84, .12, segments=8, uv=red, color=WHITE, shades=False)
    # Cone apex is one vertex: do not export degenerate zero-radius cap rings.
    start = len(p.mesh.indices)
    roof_ring = [(1.02*math.cos(math.tau*i/8), 6.44, 1.02*math.sin(math.tau*i/8)) for i in range(8)]
    for i in range(8):
        triangle(p, roof_ring[i], roof_ring[(i+1) % 8], (0, 7.44, 0), red)
        triangle(p, (0, 6.44, 0), roof_ring[(i+1) % 8], roof_ring[i], red)
    orient_outward(p.mesh, start, (0, 6.65, 0))
    solid_cylinder(p, (0, 7.43, 0), .027, .27, segments=6, uv=dark, color=WHITE, shades=False)
    finish(p, "tapered eight-face ivory lighthouse, inset red/dark windows, open gallery rail, eight-pane lantern and coral-red conical cap")


def crate(p):
    t = atlas(p)
    plank, wood = t.uv("plank", inset=2), t.uv("wood", inset=2)
    # Panels are independent planks; the substantial frames sit proud of them.
    for axis in ("x", "z"):
        for side in (-1, 1):
            for i in range(4):
                centre = ((side*.347, .12+i*.185, 0) if axis == "x" else (0, .12+i*.185, side*.347))
                size = (.05, .175, .67) if axis == "x" else (.67, .175, .05)
                box(p, centre, size, plank)
    for y in (.044, .776):
        for i in range(4):
            box(p, (-.270+i*.18, y, 0), (.171, .055, .68), plank)
    for x in (-.361, .361):
        for z in (-.361, .361):
            box(p, (x, .41, z), (.098, .82, .098), wood)
    for y in (.049, .771):
        for z in (-.361, .361):
            box(p, (0, y, z), (.63, .098, .098), wood)
        for x in (-.361, .361):
            box(p, (x, y, 0), (.098, .098, .63), wood)
    for z in (-.399, .399):
        for reverse in (False, True):
            a, b = ((-.287, .16, z), (.287, .66, z)) if not reverse else ((-.287, .66, z), (.287, .16, z))
            beam(p, a, b, .022, .075, wood)
    for x in (-.399, .399):
        beam(p, (x, .16, -.287), (x, .66, .287), .022, .075, wood)
        beam(p, (x, .66, -.287), (x, .16, .287), .022, .075, wood)
    finish(p, "thick timber edge frame, separate plank panels and full diagonal X braces on all four sides")


def dock(p):
    t = atlas(p)
    wood, plank = t.uv("wood", inset=2), t.uv("plank", inset=2)
    for x in (-2.03, 2.03):
        for z in (-.93, .93):
            box(p, (x, .8, z), (.34, 1.6, .34), wood)
    for z in (-.78, .78):
        box(p, (0, .675, z), (4.1, .22, .20), wood)
    for i in range(13):
        # Joined tops retain fitted plank grain without subpixel dark side
        # faces stippling the deck from the lookout's oblique view.
        box(p, (-1.974+i*.329, .855, 0), (.329, .15, 2.08), plank)
    for x in (-1.90, 1.90):
        beam(p, (x, .30, -.86), (x, .70, -.37), .12, .12, wood)
        beam(p, (x, .30, .86), (x, .70, .37), .12, .12, wood)
    finish(p, "walkable deck recipe at y=.93; substantial square corner posts, thirteen planks, paired underside beams and short knee braces")


def kiosk(p):
    t = atlas(p)
    wood, plank, blue = (t.uv(n, inset=2) for n in ("wood", "plank", "blue"))
    for x in (-1.04, 1.04):
        for z in (-.85, .85):
            box(p, (x, 1.55, z), (.20, 3.10, .20), wood)
    for x in (-1.04, 1.04):
        box(p, (x, 2.92, 0), (.20, .18, 1.90), wood)
    for z in (-.85, .85):
        box(p, (0, 2.92, z), (2.30, .18, .20), wood)
    # Only the rear counter blocks the kiosk; front and both side approaches are open.
    box(p, (0, .705, -.42), (1.85, .85, .70), blue)
    for x in (-.90, -.30, .30, .90):
        box(p, (x, .705, -.046), (.033, .81, .038), wood)
    box(p, (0, 1.15, -.42), (2.03, .12, .80), blue)
    box(p, (0, .13, .86), (.95, .26, .74), plank)
    hip_roof(p, 3.20, 2.90, 2.98, .66, t.uv("yellow", inset=2))
    finish(p, "open four-post timber kiosk with waist-high blue counter, broad four-slope yellow hip roof and entry step; no closed walls")


def stilt_house_blue(p):
    t = atlas(p)
    wood, plank, blue = (t.uv(n, inset=2) for n in ("wood", "plank", "blue"))
    for x in (-1.92, 1.92):
        for z in (-1.52, 0, 1.52):
            # Front posts sit inside the blue wall skin above the deck.
            # Their former +Z plane at 1.63 coincided with the wall face.
            post_z = z-.008 if z > 0 else z
            box(p, (x, 1.015, post_z), (.22, 2.03, .22), wood)
    for z in (-1.52, 0, 1.52):
        box(p, (0, .79, z), (4.12, .24, .24), wood)
    box(p, (0, .98, 0), (4.34, .16, 3.48), plank)
    # Lower storey hollow shell, with an unblocked central doorway at +Z.
    for x in (-1.97, 1.97):
        box(p, (x, 2.57, 0), (.18, 3.0, 3.18), blue)
    box(p, (0, 2.57, -1.54), (3.78, 3.0, .18), blue)
    for x in (-1.38, 1.38):
        box(p, (x, 2.57, 1.54), (1.20, 3.0, .18), blue)
    box(p, (0, 3.71, 1.54), (1.58, .72, .18), blue)
    box(p, (0, 4.08, 0), (4.18, .16, 3.34), plank)
    # Raised upper blue block is a readable second storey rather than a flat facade.
    for x in (-1.97, 1.97):
        box(p, (x, 4.65, 0), (.18, 1.04, 3.18), blue)
    for z in (-1.54, 1.54):
        box(p, (0, 4.65, z), (3.78, 1.04, .18), blue)
    for x in (-1.22, 1.22):
        window(p, x, 4.66, 1.646, .65, .75, t)
    window(p, 2.076, 4.66, .42, .68, .75, t, rotation=90)
    window(p, -2.076, 2.85, .35, .75, .9, t, rotation=-90)
    hip_roof(p, 5.40, 4.70, 5.13, .95, t.uv("yellow", inset=2))
    finish(p, "larger blue waterfront building: six square stilts, exposed underside crossbeams, raised plank floor, two blue storeys, framed windows, open front doorway and generous yellow hip roof")


def hut_brown(p):
    t = atlas(p)
    wood, plank = t.uv("wood", inset=2), t.uv("plank", inset=2)
    for x in (-1.17, 1.17):
        for z in (-1.05, 1.05):
            box(p, (x, 1.40, z), (.18, 2.80, .18), wood)
    box(p, (0, .25, 0), (2.52, .14, 2.27), plank)
    for x in (-1.17, 1.17):
        box(p, (x, 1.49, 0), (.14, 2.37, 2.12), plank)
    box(p, (0, 1.49, -1.05), (2.20, 2.37, .14), plank)
    for x in (-.90, .90):
        box(p, (x, 1.49, 1.05), (.58, 2.37, .14), plank)
    box(p, (0, 2.42, 1.05), (1.24, .51, .14), wood)
    window(p, 1.25, 1.85, .18, .64, .81, t, rotation=90)
    hip_roof(p, 3.50, 3.24, 2.68, .87, t.uv("yellow", inset=2))
    finish(p, "secondary smaller brown plank hut with four posts, elevated threshold, real open entry and warm yellow hip roof")


def umbrella(p):
    t = atlas(p)
    dark = t.uv("dark", inset=2)
    solid_cylinder(p, (0, 0, 0), .027, 2.41, segments=8, uv=dark, color=WHITE, shades=False)
    # One sealed canopy: colour seams share edges, with no doubled sector walls.
    start = len(p.mesh.indices)
    for i in range(8):
        a, b = math.tau*i/8, math.tau*(i+1)/8
        lower_a, lower_b = (1.2*math.cos(a), 2.00, 1.2*math.sin(a)), (1.2*math.cos(b), 2.00, 1.2*math.sin(b))
        upper_a, upper_b = (lower_a[0], 2.07, lower_a[2]), (lower_b[0], 2.07, lower_b[2])
        uv = t.uv("coral" if i % 2 == 0 else "ivory", inset=2)
        triangle(p, (0, 2.478, 0), upper_a, upper_b, uv)
        triangle(p, (0, 2.44, 0), lower_b, lower_a, uv)
        p.mesh.quad(lower_a, lower_b, upper_b, upper_a, uv=uv, color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices
    solid_cylinder(p, (0, 2.44, 0), .032, .11, segments=8, taper=.4, uv=dark, color=WHITE, shades=False)
    finish(p, "eight alternating coral/off-white thick canopy sectors, pointed cap, shallow valance and slender dark pole; collision is only the pole")


def lounge_chair(p):
    t = atlas(p)
    wood, blue = t.uv("wood", inset=2), t.uv("blue", inset=2)
    for x in (-.332, .332):
        beam(p, (x, .025, .67), (x, .50, -.34), .075, .075, wood)
        beam(p, (x, .025, -.50), (x, .63, .32), .075, .075, wood)
        beam(p, (x, .44, -.24), (x, 1.047, -.744), .06, .06, wood)
    for y, z in ((.44, -.24), (.48, .63), (1.047, -.744)):
        box(p, (0, y, z), (.74, .061, .061), wood)
    # Bent fabric has actual thickness and a separate reclining back.
    extrude(p, [(-.744, 1.047), (-.255, .431), (.613, .467), (.613, .487),
                (-.233, .443), (-.724, 1.069)], -.303, .303, blue, axis="x")
    finish(p, "warm folding X-frame, actual crossed legs and blue taut sling; separate sloped reclining back and seat, +Z at foot end")


def signpost(p):
    t = atlas(p)
    wood, plank = t.uv("wood", inset=2), t.uv("plank", inset=2)
    box(p, (-.27, .88, 0), (.13, 1.76, .13), wood)
    contour = [(-.60, 1.27), (.34, 1.27), (.60, 1.46), (.34, 1.65), (-.60, 1.65)]
    extrude(p, contour, -.10, .10, plank)
    finish(p, "unlettered solid timber arrow points right (+X) above a simple square post; +Z is readable front")


def town_house(p, colour):
    t = atlas(p)
    body, cream = t.uv("ivory" if colour == "coral" else colour, inset=2), t.uv("ivory", inset=2)
    name = p.id.split(":", 1)[1]
    if name == "town_house_coral":
        width, depth, wall_height = 3.12, 2.85, 3.02
        arch_width, crown, rise = 1.22, 2.26, .51
        roof_width, roof_depth, roof_rise = 3.80, 3.60, .98
    else:
        width, depth, wall_height = (3.92, 3.25, 4.46) if colour == "ivory" else (3.85, 3.42, 4.53)
        arch_width, crown, rise = 1.63, 2.39, .67
        roof_width, roof_depth, roof_rise = 4.60, 3.90, .94
    half_x, half_z = width/2, depth/2
    body_start = len(p.mesh.positions)
    for x in (-half_x+.1, half_x-.1):
        box(p, (x, wall_height/2, 0), (.20, wall_height, depth-.38), body)
    box(p, (0, wall_height/2, -half_z+.1), (width, wall_height, .20), body)
    arch_wall(p, width, wall_height, arch_width, crown, rise, .20, body, z=half_z-.1, reveal=cream)
    if colour == "coral":
        # Intentional linear albedo #f5bfaa from the ivory sheet: pale town
        # stucco differs from the board's saturated coral umbrella/crab.
        for i in range(body_start, len(p.mesh.positions)):
            p.mesh.colors[i] = (253, 159, 153)
    # Floor slab makes the open interior legible; support is supplied separately.
    box(p, (0, .075, 0), (width-.39, .15, depth-.39), t.uv("stucco", inset=2))
    for x in (-width*.30, width*.30):
        window(p, x, 3.49 if wall_height > 4 else 2.46, half_z+.01, .62, .78, t)
    window(p, half_x+.01, 3.43 if wall_height > 4 else 1.53, .0, .67, .81, t, rotation=90)
    if colour == "blue":
        box(p, (0, 4.55, 0), (4.20, .14, 3.75), t.uv("yellow", inset=2))
        for x in (-1.96, 1.96):
            box(p, (x, 4.94, 0), (.25, .82, 3.50), cream)
        box(p, (0, 4.94, -1.735), (3.68, .82, .28), cream)
        for x in (-1.41, 1.41):
            box(p, (x, 4.94, 1.735), (.84, .82, .28), cream)
        for x in (-1.96, 1.96):
            box(p, (x, 5.40, 0), (.28, .10, 3.75), cream)
    else:
        hip_roof(p, roof_width, roof_depth, wall_height, roof_rise, t.uv("yellow", inset=2))
    finish(p, f"{colour} stucco town module with genuinely hollow interior, eight-facet ground-floor doorway arch, framed upper/side windows and " + ("yellow terrace slab with cream parapet and front exit" if colour == "blue" else "broad yellow hip roof"))


def town_house_cream(p):
    town_house(p, "ivory")


def town_house_blue(p):
    town_house(p, "blue")


def town_house_coral(p):
    town_house(p, "coral")


def town_arch(p):
    t = atlas(p)
    arch_wall(p, 3.60, 2.80, 2.16, 2.45, .88, .42, t.uv("blue", inset=2), reveal=t.uv("ivory", inset=2))
    finish(p, "open blue stucco eight-facet town arch with cream reveal; 2.16 m clear portal and substantial piers")


def town_stairs(p):
    t = atlas(p)
    contour = [(-2.10, 0), (2.10, 0)]
    for i in range(12):
        z, y = 2.10-i*.35, (i+1)*.20
        contour.extend([(z, y), (z-.35, y)])
    contour = [(-z, y) for z, y in contour]
    extrude(p, contour, -.70, .70, t.uv("ivory", inset=2), axis="x")
    finish(p, "solid cream exterior staircase: twelve .20 m rises/.35 m treads, width 1.40 m; ascend from -Z to +Z to match the implemented stair primitive")


def town_terrace(p):
    t = atlas(p)
    cream = t.uv("ivory", inset=2)
    box(p, (0, .09, 0), (4.00, .18, 3.00), t.uv("stucco", inset=2))
    for x in (-1.89, 1.89):
        box(p, (x, .515, 0), (.22, .67, 2.78), cream)
        box(p, (x, .875, 0), (.25, .05, 3.00), cream)
    box(p, (0, .515, -1.39), (3.56, .67, .22), cream)
    box(p, (0, .875, -1.39), (3.50, .05, .25), cream)
    for x in (-1.42, 1.42):
        box(p, (x, .515, 1.39), (.93, .67, .22), cream)
        box(p, (x, .875, 1.39), (.93, .05, .25), cream)
    finish(p, "cream terrace deck and substantial parapets around three sides plus two front returns, leaving 1.91 m front passage; supplied floor/rail collision is separate")


def town_parapet(p):
    t = atlas(p)
    box(p, (0, .345, 0), (2.35, .69, .20), t.uv("ivory", inset=2))
    box(p, (0, .715, 0), (2.40, .05, .25), t.uv("ivory", inset=2))
    finish(p, "cream stucco parapet run with proud coping, 2.40 m modular length")


def town_terrace_base(p):
    t = atlas(p)
    cream = t.uv("ivory", inset=2)
    # One closed union: each central arch joins the full-depth side walls
    # without buried interface caps or T-junctions at their shared corners.
    curve = [(1.40*math.cos(math.pi*i/8), 1.45+.55*math.sin(math.pi*i/8))
             for i in range(9)]
    # Facet-to-top quads give each narrow arch strip one supported lightmap
    # domain. Ear clipping made a 2 m fan with a 21 mm triangle altitude.
    contour = [(-1.70, 0), (-1.70, 1.45), (-1.70, 2.22)]
    contour += [(x, 2.22) for x, _y in reversed(curve)]
    contour += [(1.70, 2.22), (1.70, 1.45), (1.70, 0), (1.40, 0)]
    contour += curve + [(-1.40, 0)]
    for low, high in ((-1.50, -1.22), (1.22, 1.50)):
        rings = [[(x, y, z) for x, y in contour] for z in (low, high)]
        for z in (low, high):
            for (ax, ay), (bx, by) in zip(curve, curve[1:]):
                p.mesh.quad((ax, ay, z), (bx, by, z),
                            (bx, 2.22, z), (ax, 2.22, z), uv=cream, color=WHITE)
            for side in (-1, 1):
                inner, outer = side*1.40, side*1.70
                for bottom, top in ((0, 1.45), (1.45, 2.22)):
                    p.mesh.quad((inner, bottom, z), (outer, bottom, z),
                                (outer, top, z), (inner, top, z), uv=cream, color=WHITE)
        for i, a in enumerate(contour):
            nxt = (i+1) % len(contour)
            b = contour[nxt]
            if abs(a[0]) == 1.70 and a[0] == b[0]:
                continue  # Internal contact with the uninterrupted side wall.
            p.mesh.quad(rings[0][i], rings[0][nxt], rings[1][nxt], rings[1][i],
                        uv=cream, color=WHITE)
    for side in (-1, 1):
        outer, inner = side*2.00, side*1.70
        for low, high in ((-1.50, -1.22), (-1.22, 1.22), (1.22, 1.50)):
            # Splitting these skins at the arch edges avoids T-junctions.
            for bottom, top in ((0, 1.45), (1.45, 2.22)):
                p.mesh.quad((outer, bottom, low), (outer, top, low),
                            (outer, top, high), (outer, bottom, high), uv=cream, color=WHITE)
            for y in (0, 2.22):
                p.mesh.quad((inner, y, low), (outer, y, low),
                            (outer, y, high), (inner, y, high), uv=cream, color=WHITE)
        for bottom, top in ((0, 1.45), (1.45, 2.22)):
            p.mesh.quad((inner, bottom, -1.22), (inner, top, -1.22),
                        (inner, top, 1.22), (inner, bottom, 1.22), uv=cream, color=WHITE)
            for z in (-1.50, 1.50):
                p.mesh.quad((inner, bottom, z), (outer, bottom, z),
                            (outer, top, z), (inner, top, z), uv=cream, color=WHITE)
    inspect(p.mesh.positions, p.mesh.indices, repair=True)
    finish(p, "continuous cream arched lookout base; open 2.8 m passage, 2 m crown and grounded side walls support the 2.22 m deck")


def bunting(p):
    t = atlas(p)
    dark = t.uv("wood", inset=2)
    points = [(-2.19+i*.438, .50-.115*math.sin(math.pi*i/10), 0) for i in range(11)]
    for a, b in zip(points, points[1:]):
        beam(p, a, b, .022, .022, dark)
    for i in range(8):
        x = -1.785+i*.51
        y = .50-.115*math.sin(math.pi*(x+2.19)/4.38)
        if i % 2 == 0:
            # Intentional local albedo: authored yellow multiplied to the board's orange.
            p.begin_material(p.material("bunting_orange", color=(235, 135, 244)))
            uv = t.uv("yellow", inset=2)
        else:
            p.begin_material(p.material("bunting_blue"))
            uv = t.uv("blue", inset=2)
        extrude(p, [(x-.18, y), (x+.18, y), (x+.145, y-.285), (x-.055, y-.32)], -.012, .012, uv)
    finish(p, "gently sagging timber-colour cord and eight thick orange/blue cloth pennants; no alpha cards or extra decoration")


PROPS = {"beach:" + name: build for name, build in {
    "lighthouse": lighthouse, "crate": crate, "dock": dock, "kiosk": kiosk,
    "stilt_house_blue": stilt_house_blue, "hut_brown": hut_brown,
    "umbrella": umbrella, "lounge_chair": lounge_chair, "signpost": signpost,
    "town_house_cream": town_house_cream, "town_house_blue": town_house_blue,
    "town_house_coral": town_house_coral, "town_arch": town_arch,
    "town_stairs": town_stairs, "town_terrace": town_terrace,
    "town_terrace_base": town_terrace_base,
    "town_parapet": town_parapet, "bunting": bunting,
}.items()}


def catalog_entries():
    """Primary-owned registration input; this helper never rewrites the catalog."""
    return [dict(id="beach:"+name, display_name="Beach "+name.replace("_", " ").title(),
                 asset_class="environment", theme="beach", asset_type="prop", source="file",
                 model=f"environment/beach/props/models/{name}.glb", size=list(size),
                 color="#328aca" if "blue" in name else "#ead2a8", category="Beach Structures",
                 solid=name in ("crate", "lounge_chair", "town_parapet"))
            for name, size in SIZES.items()]


# Each box is (local centre x, base y, centre z, width, height, depth).
# These are geometric specifications, NOT one enclosing solid box for a hollow prop.
# Supply them as separate authored solids, with the tiny existing collision_peg
# buried inside each visible timber/stucco mass and occludes:false. Prop Y is
# floor-relative: resolve the local supporting floor before translating base Y.
# Roof overhangs do not block empty walk-through space. Usable deck floors and
# staircase traversal must also use the matching support recipe below.
COLLISION_BOXES = {
    "lighthouse": [(0, 0, 0, 1.88, 4.98, 1.88)],
    "signpost": [(-.27, 0, 0, .13, 1.27, .13), (0, 1.27, 0, 1.20, .38, .20)],
    "dock": [(x, 0, z, .34, 1.60, .34) for x in (-2.03, 2.03) for z in (-.93, .93)] +
            [(0, .78, 0, 4.10, .15, 2.08)],
    "kiosk": [(x, 0, z, .20, 3.10, .20) for x in (-1.04, 1.04) for z in (-.85, .85)] +
             [(0, .28, -.42, 2.03, .93, .80), (0, 0, .86, .95, .26, .74)],
    "stilt_house_blue": [(x, 0, z, .22, 2.03, .22) for x in (-1.92, 1.92) for z in (-1.52, 0, 1.52)] +
                        [(0, .90, 0, 4.34, .16, 3.48), (-1.97, 1.07, 0, .18, 3., 3.18),
                         (1.97, 1.07, 0, .18, 3., 3.18), (0, 1.07, -1.54, 3.78, 3., .18),
                         (-1.38, 1.07, 1.54, 1.20, 3., .18), (1.38, 1.07, 1.54, 1.20, 3., .18),
                         (0, 3.35, 1.54, 1.58, .72, .18)],
    "hut_brown": [(-1.17, .305, 0, .14, 2.37, 2.12), (1.17, .305, 0, .14, 2.37, 2.12),
                  (0, .305, -1.05, 2.20, 2.37, .14), (-.90, .305, 1.05, .58, 2.37, .14),
                  (.90, .305, 1.05, .58, 2.37, .14), (0, 2.165, 1.05, 1.24, .51, .14),
                  (0, .18, 0, 2.52, .14, 2.27)],
    "umbrella": [(0, 0, 0, .054, 2.03, .054)],
    "town_stairs": [(0, 0, -1.925+i*.35, 1.40, (i+1)*.20, .35) for i in range(12)],
    "town_terrace": [(0, 0, 0, 4., .18, 3.), (-1.89, .18, 0, .22, .72, 2.78),
                     (1.89, .18, 0, .22, .72, 2.78), (0, .18, -1.39, 3.56, .72, .22),
                     (-1.42, .18, 1.39, .93, .72, .22), (1.42, .18, 1.39, .93, .72, .22)],
}


def _arch_collision_boxes(width, height, opening, crown, rise, depth, *, z=0):
    """Stock-inset boxes following the actual eight-facet doorway contour.

    The engine ArchwayDef's full-width header starts at the spring height;
    these visual arches need narrower headers to preserve standing passage.
    Four strips per facet use the higher endpoint as their base, keeping each
    blocker and its tiny carrier inside the visible sloping wall stock.
    """
    radius, spring = opening/2, crown-rise
    pier_width = (width-opening)/2
    boxes = [[side*(width+opening)/4, .003, z, pier_width-.006, height-.006, depth-.012]
             for side in (-1, 1)]
    curve = [(radius*math.cos(math.pi*i/8), spring+rise*math.sin(math.pi*i/8))
             for i in range(9)]
    for (ax, ay), (bx, by) in zip(curve, curve[1:]):
        for i in range(4):
            left = (ax+(bx-ax)*i/4, ay+(by-ay)*i/4)
            right = (ax+(bx-ax)*(i+1)/4, ay+(by-ay)*(i+1)/4)
            base = max(left[1], right[1])+.003
            boxes.append([(left[0]+right[0])/2, base, z, abs(right[0]-left[0]),
                          height-.003-base, depth-.012])
    return boxes


def structural_components(name):
    """Return local, scale-1 structural recipes for the Phase 3 map author.

    ``collision_boxes`` are standable prop blockers in local coordinates.
    ``support`` supplies real floor regions/stairs, rather than claiming a GLB
    automatically creates floors. Doorway piers and narrow curved-header boxes
    preserve the visible arch opening; no ArchwayDef shortcut fills the space
    above the spring. Translate/rotate with the visual instance; insert into a
    valid room and resolve all absolute structural Y values.
    """
    if name.startswith("beach:"):
        name = name.split(":", 1)[1]
    result = {"collision_boxes": [list(b) for b in COLLISION_BOXES.get(name, [])]}
    if name == "dock":
        result["support"] = dict(kind="floor_region", x=-2.05, z=-1.04, width=4.10, depth=2.08,
                                 surface_y=.93, edge_material="beach:wood_plank_01")
    elif name == "town_stairs":
        result["support"] = dict(kind="stairs", x=-.70, z=-2.10, width=1.40, depth=4.20,
                                 offset_y=0, rise=2.40, steps=12, material="beach:stucco_01")
    elif name == "town_terrace":
        result["support"] = dict(kind="floor_region", x=-2., z=-1.5, width=4., depth=3.,
                                 surface_y=.18, edge_material="beach:stucco_01")
    elif name == "town_terrace_base":
        result["collision_boxes"] = [
            b for z in (-1.36, 1.36)
            for b in _arch_collision_boxes(4., 2.22, 2.80, 2., .55, .28, z=z)]
        result["collision_boxes"] += [[x, 0, 0, .294, 2.217, 2.434] for x in (-1.85, 1.85)]
    elif name in ("town_house_cream", "town_house_blue", "town_house_coral", "town_arch"):
        width, depth, height, opening, crown, rise = {
            "town_house_cream": (3.92, 3.25, 4.46, 1.63, 2.39, .67),
            "town_house_blue": (3.85, 3.42, 4.53, 1.63, 2.39, .67),
            "town_house_coral": (3.12, 2.85, 3.02, 1.22, 2.26, .51),
            "town_arch": (3.60, .42, 2.80, 2.16, 2.45, .88),
        }[name]
        z = 0 if name == "town_arch" else depth/2-.1
        thick = .42 if name == "town_arch" else .20
        result["collision_boxes"] = _arch_collision_boxes(width, height, opening, crown, rise, thick, z=z)
        if name != "town_arch":
            result["collision_boxes"] += [[-width/2+.1, 0, 0, .20, height, depth-.4],
                                          [width/2-.1, 0, 0, .20, height, depth-.4],
                                          [0, 0, -depth/2+.1, width, height, .20]]
            result["support"] = dict(kind="floor_region", x=-width/2+.2, z=-depth/2+.2,
                                     width=width-.4, depth=depth-.4, surface_y=.15,
                                     edge_material="beach:stucco_01")
    return result


def placed_components(name, x, z, *, base_y=0, floor_y=0, rotation_degrees=0, identity=None,
                      floor_at=None):
    """Emit directly usable v3 arrays for a visual module and its real support.

    Coordinates ``x,z,base_y`` locate the model's base in world space; ``floor_y``
    is the containing room's absolute floor Y (the region-offset reference).
    Optional ``floor_at(world_x, world_z)`` supplies the existing supported floor
    including pre-existing regions/ramps; the default is the room's ``floor_y``.
    Quarter turns are supported. The returned props already compensate their
    floor-relative Y for those regions. Structural surfaces are buried 3–6 mm
    inside the GLB stock, preserving collision without duplicate visible faces.
    Hollow doorway arches use separate piers/curved-header collider props and
    leave the returned archways array empty, preserving standing headroom.
    Caller owns the containing room, approaches/landings and instance namespace.
    """
    if name.startswith("beach:"):
        name = name.split(":", 1)[1]
    if name not in SIZES:
        raise ValueError(f"unknown Beach structure {name}")
    if not all(math.isfinite(v) for v in (x, z, base_y, floor_y, rotation_degrees)):
        raise ValueError("Beach placement coordinates must be finite")
    if floor_at is not None and not callable(floor_at):
        raise ValueError("floor_at must be a callable world-floor query")
    yaw = rotation_degrees % 360
    quarter = round(yaw / 90) % 4
    if abs((yaw-quarter*90+180) % 360-180) > 1e-6:
        raise ValueError("Beach structural placement supports quarter-turn rotations")
    identity = identity or "beach_"+name
    result = {"props": [], "floor_regions": [], "stairs": [], "archways": []}

    def point(px, pz):
        return ((x+px, z+pz), (x+pz, z-px), (x-px, z-pz), (x-pz, z+px))[quarter]

    def rectangle(px, pz, width, depth):
        corners = [point(a, b) for a in (px, px+width) for b in (pz, pz+depth)]
        min_x, max_x = min(q[0] for q in corners), max(q[0] for q in corners)
        min_z, max_z = min(q[1] for q in corners), max(q[1] for q in corners)
        return dict(x=min_x, z=min_z, width=max_x-min_x, depth=max_z-min_z)

    def region(px, pz, width, depth, surface_y, material):
        result["floor_regions"].append({**rectangle(px, pz, width, depth),
                                        "offset_y": base_y+surface_y-floor_y,
                                        "material": material, "edge_material": material})

    local = structural_components(name)
    support = local.get("support")
    if support and support["kind"] == "floor_region":
        material = "beach:wood_plank_01" if name == "dock" else "beach:stucco_01"
        region(support["x"], support["z"], support["width"], support["depth"],
               support["surface_y"]-.005, material)
    elif name == "town_stairs":
        # Regions support both axes/descending world runs without an invented
        # StairDef direction key. Every GLB tread sits 5 mm above its support.
        for i in range(12):
            region(-.70, -2.10+i*.35, 1.40, .35, (i+1)*.20-.005, "beach:stucco_01")
    elif name == "stilt_house_blue":
        region(-2.17, -1.74, 4.34, 3.48, 1.055, "beach:wood_plank_01")
    elif name == "hut_brown":
        region(-1.26, -1.135, 2.52, 2.27, .315, "beach:wood_plank_01")

    def supporting_floor(px, pz):
        height = floor_y if floor_at is None else floor_at(px, pz)
        if height is None or not math.isfinite(height):
            raise ValueError(f"Beach placement has no finite supporting floor at {px},{pz}")
        for authored in result["floor_regions"]:
            if (authored["x"]-1e-8 <= px <= authored["x"]+authored["width"]+1e-8 and
                    authored["z"]-1e-8 <= pz <= authored["z"]+authored["depth"]+1e-8):
                height = floor_y+authored["offset_y"]
        return height

    result["props"].append(dict(id=identity, model="beach:"+name, x=x, z=z,
                                y=base_y-supporting_floor(x, z), rotation_degrees=yaw,
                                size=list(SIZES[name]), solid=False))
    proxy_scale = .05  # Existing 6 cm carrier becomes a hidden 3 mm cube.
    for i, (px, py, pz, width, height, depth) in enumerate(local["collision_boxes"]):
        wx, wz = point(px, pz)
        result["props"].append(dict(id=f"{identity}_solid_{i}", model="outdoor:collision_peg",
                                    x=wx, z=wz, y=base_y+py-supporting_floor(wx, wz),
                                    rotation_degrees=yaw, scale=proxy_scale,
                                    size=[width/proxy_scale, height/proxy_scale, depth/proxy_scale],
                                    solid=True, occludes=False))
    if name in ("crate", "lounge_chair", "town_parapet"):
        # Crate/parapet are solid stock. Chair's standable mass is its low seat;
        # the inclined back never creates a high invisible landing plane.
        result["props"][0]["solid"] = True
        if name == "lounge_chair":
            result["props"][0]["size"] = [.70, .48, 1.42]
    return result

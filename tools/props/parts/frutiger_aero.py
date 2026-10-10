"""Faithful Frutiger Aero modules; closed faceted geometry, committed atlas.

Metres, +Y up and +Z front. Hollow architecture is visual geometry: use the
separate local structural recipes below instead of a box across an aperture.
The shared 256 px atlas is loaded, never painted by a normal model build.
"""
from __future__ import annotations

import math
from pathlib import Path

from geometry import inspect
from parts.beach_structures import beam, extrude, loft
from parts.refreshed import load_atlas_from, solid_box, solid_cylinder

ROOT = Path(__file__).resolve().parents[3] / "assets/environment/frutiger_aero/props/models"
WHITE = (255, 255, 255)
PALETTE = (
    "white", "aqua", "cyan", "lime", "mint", "silver", "branch", "foliage",
    "upholstery", "dark", "foliage_light", "foliage_dark", "water", "sky",
    "light_cyan", "warm_light",
)
GRAPHICS = {
    "kiosk": (0, 64, 64, 128), "atrium": (64, 64, 64, 128),
    "corridor": (128, 64, 64, 128), "reception": (192, 64, 64, 128),
    "accent": (0, 192, 32, 64), "leaf": (32, 192, 64, 64),
    "ring": (96, 192, 64, 64), "terminal": (160, 192, 96, 64),
}
SIZES = {
    "wall_bay": (3.6, 3.4, .556), "wall_bay_tall": (3.6, 4.2, .556),
    "lime_arch": (3.6, 3.4, .463), "glass_partition": (3.495, 2.5, .695),
    "light_pod": (2.4, 3.6, 2.4), "tree_planter": (1.586167, 3.008870, 1.628473),
    "seating_pod": (2.4, 1.04, 1.1), "display_kiosk": (1.0, 2.4, .597),
    "fountain_basin": (3.2, .46, 3.2), "bubble_sculpture": (1.351268, 2.70, 1.236373),
    "glass_canopy": (3.2, 3.4, 3.6), "accent_panel": (1.1, 2.2, .07),
    "atrium_dome": (12.0, 2.447466, 12.0), "ceiling_ring": (1.3, .13, 1.3),
    "double_doors": (2.8, 3.2, .34), "double_doors_open": (2.8, 3.2, 1.345823),
    "reception_counter": (4.5, 1.38, 1.477571), "reception_soffit": (5.2, .34, 2.1),
    "banner_atrium": (1.2, 2.4, .06), "banner_corridor": (1.2, 2.4, .06),
    "banner_reception": (1.2, 2.4, .06), "city_backdrop": (23.85, 8.6, 2.841),
    "green_backdrop": (24.0, 1.3, 5.0),
}


def atlas(p):
    p.ao_strength = 0
    t = load_atlas_from(p, ROOT / "frutiger_aero_atlas.png", ())
    for i, name in enumerate(PALETTE):
        t.region(name, (i % 8 * 32, i // 8 * 32, 32, 32))
    for name, rect in GRAPHICS.items():
        t.region(name, rect)
    return t


def graphic_atlas(p, filename, face):
    """Dedicated native graphic stock: a true 128 x 256 face plus swatches.

    The tall face occupies the left half without changing its 1:2 UV contract;
    the right half keeps the housing colours on the same embedded PNG.
    """
    p.ao_strength = 0
    t = load_atlas_from(p, ROOT / filename, ())
    for name, rect in {
        face: (0, 0, 128, 256), "white": (128, 0, 64, 64),
        "lime": (192, 0, 64, 64), "cyan": (128, 64, 64, 64),
        "silver": (192, 64, 64, 64),
    }.items():
        t.region(name, rect)
    return t


def material(p, name="ceramic", **kwargs):
    settings = {"roughness": .30, **kwargs}
    slot = p.material("aero_" + name, **settings)
    p.begin_material(slot)
    return slot


def box(p, center, size, uv):
    solid_box(p, center, size, uv=uv, color=WHITE, shade=False)


def cylinder(p, center, radius, height, uv, segments=16, **kwargs):
    solid_cylinder(p, center, radius, height, segments=segments,
                   uv=uv, color=WHITE, shades=False, **kwargs)


def finish(p, note):
    low, high = p.mesh.bounds()
    p.author_origin_shift = ((low[0]+high[0])/2, low[1], (low[2]+high[2])/2)
    p.mesh.normalize_origin()
    inspect(p.mesh.positions, p.mesh.indices, repair=True)
    report = inspect(p.mesh.positions, p.mesh.indices)
    invalid = ("degenerate", "boundary_edges", "nonmanifold_edges", "inconsistent_edges",
               "flipped_triangles", "contradictory_components")
    if any(report[key] for key in invalid):
        raise ValueError(f"{p.id}: invalid closed Aero topology: {report}")
    p.add_note(note)
    p.mesh.validate(p.size)


def rounded_outline(width, depth, radius, segments=3):
    """Counter-clockwise X/Z rounded rectangle without duplicated vertices."""
    radius = min(radius, width/2, depth/2)
    result = []
    for x, z, start in ((width/2-radius, depth/2-radius, 0),
                        (-width/2+radius, depth/2-radius, math.pi/2),
                        (-width/2+radius, -depth/2+radius, math.pi),
                        (width/2-radius, -depth/2+radius, 3*math.pi/2)):
        for step in range(segments+1):
            angle = start + step * math.pi/2 / segments
            result.append((x+radius*math.cos(angle), z+radius*math.sin(angle)))
    return result


def rounded_box(p, center, size, uv, radius=.12, segments=3):
    """Real rounded stock with sealed top and bottom; no overlaid flat cards."""
    cx, cy, cz = center
    w, h, d = size
    rings = [[(cx+x, y, cz+z) for x, z in rounded_outline(w, d, radius, segments)]
             for y in (cy-h/2, cy+h/2)]
    loft(p, rings, uv)


def ring(p, outer, inner, y, height, uv, segments=16):
    """Closed annular stock, preserving the true central hole."""
    start = len(p.mesh.indices)
    rows = [[(r*math.cos(math.tau*i/segments), yy, r*math.sin(math.tau*i/segments))
             for i in range(segments)] for yy, r in
            ((y, outer), (y+height, outer), (y, inner), (y+height, inner))]
    for i in range(segments):
        j = (i+1) % segments
        for a, b in ((0, 1), (2, 3), (0, 2), (1, 3)):
            p.mesh.quad(rows[a][i], rows[a][j], rows[b][j], rows[b][i], uv=uv, color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices


def leaf(p, center, width, height, depth, uv, *, angle=0, yaw=0):
    """Ten-triangle folded angular leaf; front/back are sealed by the crease."""
    outline = [(-width*.47, -height*.10), (-width*.24, height*.30),
               (width*.45, height*.50), (width*.30, -height*.27),
               (-width*.25, -height*.50)]
    ca, sa = math.cos(angle), math.sin(angle)
    front = []
    for x, y in outline:
        xx, yy = x*ca-y*sa, x*sa+y*ca
        front.append((center[0]+xx*math.cos(yaw), center[1]+yy,
                      center[2]-xx*math.sin(yaw)))
    ridges = [(center[0]+d*math.sin(yaw), center[1], center[2]+d*math.cos(yaw))
              for d in (depth, -depth)]
    start = len(p.mesh.indices)
    for ridge in ridges:
        for i, point in enumerate(front):
            nxt = front[(i+1) % len(front)]
            u0, v0, u1, v1 = uv
            p.mesh.triangle(ridge, point, nxt,
                            [(u0+(u1-u0)/2, v0+(v1-v0)/2), (u0, v1), (u1, v0)], WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices


def _frame_contour(width, crown, jamb, inner_width, inner_crown, inner_jamb, segments=8):
    outer = [(-width/2, 0), (-width/2, jamb)]
    outer += [(width/2*math.cos(math.pi-i*math.pi/segments),
               jamb+(crown-jamb)*math.sin(math.pi-i*math.pi/segments))
              for i in range(1, segments+1)]
    outer += [(width/2, 0), (inner_width/2, 0), (inner_width/2, inner_jamb)]
    outer += [(inner_width/2*math.cos(i*math.pi/segments),
               inner_jamb+(inner_crown-inner_jamb)*math.sin(i*math.pi/segments))
              for i in range(1, segments+1)]
    outer += [(-inner_width/2, 0)]
    return outer


def _wall_bay(p, tall=False):
    t = atlas(p)
    h = 4.2 if tall else 3.4
    material(p)
    # A broad rounded frame with a folded returning right end, not a flat wall.
    outer = rounded_outline(3.6, h, .20, 3)
    inner = rounded_outline(2.98, h-.70, .12, 3)
    outer = [(x, y+h/2) for x, y in outer]
    inner = [(x, y+h/2) for x, y in inner]
    start = len(p.mesh.indices)
    for z in (-.10, .13):
        for i in range(len(outer)):
            j = (i+1) % len(outer)
            p.mesh.quad((*outer[i], z), (*outer[j], z), (*inner[j], z), (*inner[i], z),
                        uv=t.uv("white", 3), color=WHITE)
    for contour in (outer, inner):
        for i in range(len(contour)):
            j = (i+1) % len(contour)
            p.mesh.quad((*contour[i], -.10), (*contour[j], -.10),
                        (*contour[j], .13), (*contour[i], .13), uv=t.uv("white", 3), color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices
    rounded_box(p, (1.68, h/2, -.145), (.24, h-.02, .55), t.uv("white", 3), .10, 2)
    # Sparse real construction joints keep the broad frame readable.
    material(p, "trim", roughness=.48, metallic=.18)
    for x in (-1.16, .36, 1.13):
        for y in (.175, h-.175):
            box(p, (x, y, .132), (.008, .25, .008), t.uv("silver", 3))
    material(p, "leaf", roughness=.76)
    leaf(p, (.40, h*.50, .038), .85, 1.34, .008, t.uv("foliage", 3), angle=-.20)
    beam(p, (.07, h*.30, .035), (.59, h*.69, .035), .023, .008, t.uv("foliage_dark", 3))
    material(p, "glass", alpha_mode="blend", opacity=.17, roughness=.13)
    box(p, (0, h/2, 0), (3.04, h-.65, .024), t.uv("cyan", 3))
    finish(p, "A01: rounded white glazed bay, panel joints, returning end and folded leaf; hollow/frame collision")


def wall_bay(p):
    _wall_bay(p)


def wall_bay_tall(p):
    _wall_bay(p, True)


def lime_arch(p):
    t = atlas(p)
    material(p)
    extrude(p, _frame_contour(3.6, 3.4, 1.84, 2.97, 3.08, 1.84), -.225, .225, t.uv("white", 3))
    material(p, "lime_reveal", roughness=.33)
    extrude(p, _frame_contour(2.98, 3.085, 1.84, 2.54, 2.85, 1.82), -.16, .18, t.uv("lime", 3))
    material(p, "base_aqua", roughness=.28)
    for x in (-1.66, 1.66):
        box(p, (x, .30, .229), (.13, .42, .018), t.uv("aqua", 3))
    finish(p, "A02: eight-facet round crown, straight jambs, lime inner band and aqua base inserts; 2.54 m clear opening")


def glass_partition(p):
    t = atlas(p)
    # Three panels folded in a shallow, deliberately legible zigzag.
    endpoints = [(-1.72, .32), (-.58, -.32), (.58, .32), (1.72, -.32)]
    material(p)
    for x, z in endpoints:
        box(p, (x, 1.25, z), (.055, 2.5, .055), t.uv("white", 3))
    for a, b in zip(endpoints, endpoints[1:]):
        for y in (.045, 2.455):
            beam(p, (a[0], y, a[1]), (b[0], y, b[1]), .055, .07, t.uv("white", 3))
    material(p, "leaf", roughness=.76)
    for i, (a, b) in enumerate(zip(endpoints, endpoints[1:])):
        center = ((a[0]+b[0])/2, .97+(.12 if i==1 else 0), (a[1]+b[1])/2+.03)
        leaf(p, center, (.44, .55, .76)[i], (.84, 1.0, 1.30)[i], .014,
             t.uv("foliage" if i%2==0 else "lime", 3), angle=(-.21, .30, -.24)[i])
    material(p, "glass", alpha_mode="blend", opacity=.17, roughness=.13)
    for a, b in zip(endpoints, endpoints[1:]):
        vx, vz = b[0]-a[0], b[1]-a[1]
        length = math.hypot(vx, vz)
        nx, nz = -vz/length*.01, vx/length*.01
        rings = [[(a[0]+nx*s, .075, a[1]+nz*s), (b[0]+nx*s, .075, b[1]+nz*s),
                  (b[0]+nx*s, 2.425, b[1]+nz*s), (a[0]+nx*s, 2.425, a[1]+nz*s)] for s in (-1, 1)]
        loft(p, rings, t.uv("cyan", 3))
    finish(p, "A03: exactly three folded cyan panes, thin white joints/rails and differently sized nature leaves")


def light_pod(p):
    t = atlas(p)
    material(p)
    ring(p, 1.20, .95, .08, .23, t.uv("white", 3), 8)
    cylinder(p, (0, .16, 0), .95, .09, t.uv("white", 3), 8)
    material(p, "suspension", roughness=.5, metallic=.25)
    for angle in (math.pi/2, math.pi*7/6, math.pi*11/6):
        x, z = .88*math.cos(angle), .88*math.sin(angle)
        beam(p, (x, .30, z), (x, 3.6, z), .018, .018, t.uv("silver", 3))
    material(p, "cyan_light", emissive=(.62, .95, 1), strength=1.8, roughness=.16)
    ring(p, 1.10, .89, 0, .035, t.uv("light_cyan", 3), 8)
    finish(p, "A04: wide octagonal suspended pod, recessed pale center, luminous cyan underside ring and three suspension lines; emitter below base")


def tree_planter(p):
    t = atlas(p)
    material(p)
    rounded_box(p, (0, .365, 0), (1.28, .57, 1.28), t.uv("white", 3), .16, 3)
    material(p, "aqua_base", roughness=.31)
    rounded_box(p, (0, .075, 0), (1.26, .15, 1.26), t.uv("aqua", 3), .15, 3)
    material(p, "soil", roughness=.95)
    rounded_box(p, (0, .646, 0), (1.05, .025, 1.05), t.uv("branch", 3), .10, 2)
    material(p, "branch", roughness=.88)
    trunk = [(0, .65, 0), (-.05, 1.10, .02), (.03, 1.63, -.02), (-.01, 2.22, .01)]
    rings = [[(x+r*math.cos(math.tau*i/6), y, z+r*math.sin(math.tau*i/6))
              for i in range(6)] for (x, y, z), r in zip(trunk, (.09, .075, .061, .035))]
    loft(p, rings, t.uv("branch", 3))
    branches = [((-.02, 1.35, 0), (-.49, 2.16, -.13)), ((0, 1.72, 0), (.50, 2.41, .13)),
                ((-.04, 1.51, 0), (.27, 2.12, -.40)), ((0, 1.85, 0), (-.32, 2.63, .29)),
                ((-.03, 1.94, 0), (.23, 2.84, .03))]
    for a, b in branches:
        beam(p, a, b, .062, .053, t.uv("branch", 3))
    material(p, "foliage", roughness=.82)
    # Unequal folded individual leaves retain the reference's angular canopy.
    for i in range(36):
        angle = math.tau*i/13 + (i//13)*.53
        radius = (.48, .64, .40)[i//13]
        y = 2.13 + (i//13)*.30 + .075*math.sin(i*2.17)
        center = (radius*math.cos(angle), y, radius*math.sin(angle))
        region = ("foliage", "foliage_light", "foliage_dark", "lime")[i%4]
        leaf(p, center, .34+.035*math.sin(i), .42+.055*math.sin(i*1.4), .055,
             t.uv(region, 3), angle=angle*.37, yaw=angle)
    for i in range(8):
        angle = math.tau*i/8
        leaf(p, (.35*math.cos(angle), .79, .35*math.sin(angle)), .25, .39, .055,
             t.uv("foliage_light" if i%2 else "foliage", 3), angle=angle, yaw=angle)
    finish(p, "A05: rounded square white planter/aqua base; bent brown trunk with five branches, 36 angular canopy leaves and eight basal leaves")


def seating_pod(p):
    t = atlas(p)
    material(p)
    # Continuous rounded U shell wraps around the two cushions and stays open.
    outer = [(-1.2, .55), (-1.2, -.25), (-1.16, -.40), (-1.04, -.51),
             (-.86, -.55), (.86, -.55), (1.04, -.51), (1.16, -.40),
             (1.2, -.25), (1.2, .55), (.97, .55), (.97, -.20),
             (.93, -.29), (.84, -.33), (-.84, -.33), (-.93, -.29),
             (-.97, -.20), (-.97, .55)]
    # Extrusion along Y: translate the X/Z footprint into the existing X/Y helper.
    start = len(p.mesh.indices)
    extrude(p, outer, .23, 1.04, t.uv("white", 3))
    for i in range(min(p.mesh.indices[start:]), len(p.mesh.positions)):
        x, z, y = p.mesh.positions[i]
        if y > 1:
            # Back remains 1.04 m; the arms descend to the reference's .70 m
            # front ledge, with continuous stock over their rounded returns.
            y = 1.04 if z <= -.25 else .70 if z >= .15 else 1.04-(z+.25)/.40*.34
        p.mesh.positions[i] = (x, y, z)
    rounded_box(p, (0, .405, .045), (2.17, .23, .92), t.uv("white", 3), .16, 3)
    for x in (-.85, .85):
        for z in (-.32, .32):
            rounded_box(p, (x, .13, z), (.20, .26, .18), t.uv("white", 3), .04, 2)
    material(p, "upholstery", roughness=.57)
    for x in (-.48, .48):
        rounded_box(p, (x, .565, .075), (.935, .17, .79), t.uv("upholstery", 3), .10, 3)
        rounded_box(p, (x, .825, -.245), (.935, .41, .155), t.uv("upholstery", 3), .065, 2)
    finish(p, "A06: continuous curved white wraparound shell; two cyan seat/back cushions, recessed underside and four short feet; front +Z")


def display_kiosk(p):
    t = graphic_atlas(p, "kiosk_atlas.png", "kiosk")
    material(p)
    rounded_box(p, (0, .35, 0), (1.0, .70, .58), t.uv("white", 3), .07, 2)
    rounded_box(p, (0, 1.535, -.055), (.84, 1.73, .19), t.uv("white", 3), .035, 2)
    material(p, "lime_detail", roughness=.33)
    box(p, (-.438, 1.515, -.042), (.025, 1.47, .12), t.uv("lime", 3))
    box(p, (-.345, .35, .298), (.24, .41, .018), t.uv("lime", 3))
    material(p, "display", roughness=.22)
    box(p, (0, 1.525, .049), (.74, 1.48, .012), {"+z": t.uv("kiosk", 1),
                                                    **{f: t.uv("cyan", 3) for f in ("+x", "-x", "+y", "-y", "-z")}})
    finish(p, "A07: cyan leaf/text information display, white squat plinth and lime side inset; fitted 1:2 typography face")


def fountain_basin(p):
    t = atlas(p)
    material(p)
    # Real shallow hollow basin: rim remains open for authored circular water.
    ring(p, 1.60, 1.32, .07, .39, t.uv("white", 3), 20)
    cylinder(p, (0, 0, 0), 1.47, .10, t.uv("white", 3), 20)
    material(p, "basin_aqua", roughness=.28)
    ring(p, 1.575, 1.45, .025, .075, t.uv("aqua", 3), 20)
    finish(p, "A08 basin: 20-facet shallow white ring, recessed real floor, aqua base stripe; water radius1.30 surfaceY.37 swimming false")


def _sphere(p, center, radius, tex, segments=12, bands=5):
    rings = []
    for j in range(1, bands):
        angle = math.pi*j/bands
        y, r = center[1]+radius*math.cos(angle), radius*math.sin(angle)
        rings.append([(center[0]+r*math.cos(math.tau*i/segments), y,
                       center[2]+r*math.sin(math.tau*i/segments)) for i in range(segments)])
    start = len(p.mesh.indices)
    for j, (upper, lower) in enumerate(zip(rings, rings[1:])):
        for i in range(segments):
            nxt = (i+1) % segments
            # Different clean colour facets retain the sheet's triangular
            # glass vocabulary instead of reading as vertical block stripes.
            for face, vertices in enumerate(((upper[i], lower[i], lower[nxt]),
                                              (upper[i], lower[nxt], upper[nxt]))):
                region = ("sky", "cyan", "light_cyan", "sky", "mint", "cyan")[(i*7+j*3+face)%6]
                uv = tex.uv(region, 3)
                u0, v0, u1, v1 = uv
                p.mesh.triangle(*vertices, [(u0, v0), (u0, v1), (u1, v1)], WHITE)
    for y, ring_points in ((center[1]+radius, rings[0]), (center[1]-radius, rings[-1])):
        for i in range(segments):
            uv = tex.uv("sky" if i%3 else "light_cyan", 3)
            u0, v0, u1, v1 = uv
            p.mesh.triangle((center[0], y, center[2]), ring_points[i], ring_points[(i+1)%segments],
                            [((u0+u1)/2, v0), (u0, v1), (u1, v1)], WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices


def bubble_sculpture(p):
    t = atlas(p)
    material(p, "leaf", roughness=.75)
    for center, angle in (((-.42, .54, .20), -.30), ((.43, .49, .13), .34)):
        leaf(p, center, .38, .63, .035, t.uv("lime", 3), angle=angle)
    # No inner sphere/cubemap shell: a single closed faceted surface per bubble.
    material(p, "bubble", alpha_mode="blend", opacity=.55, roughness=.055)
    for center, radius in (((.10, .55, .02), .55), ((-.11, 1.44, -.015), .65),
                           ((.11, 2.38, .035), .32)):
        _sphere(p, center, radius, t)
    finish(p, "A08 sculpture: three individually closed stacked faceted cyan bubbles with fresh green leaf detail; one surface per sphere, no nested shells")


def glass_canopy(p):
    t = atlas(p)
    material(p)
    frame = _frame_contour(3.2, 3.4, 2.13, 2.88, 3.24, 2.12, 8)
    for z in (-1.80, 1.66):
        extrude(p, frame, z, z+.14, t.uv("white", 3))
    for x in (-1.5, 1.5):
        for y in (.065, 2.145):
            box(p, (x, y, 0), (.11, .10, 3.43), t.uv("white", 3))
    material(p, "leaf", roughness=.75)
    # Leaf lies in the side glazing, facing +X. Its silhouette has real depth.
    first = len(p.mesh.positions)
    leaf(p, (0, 1.04, 0), .84, 1.53, .015, t.uv("foliage", 3), angle=-.24)
    for i in range(first, len(p.mesh.positions)):
        x, y, z = p.mesh.positions[i]
        p.mesh.positions[i] = (1.452+z, y, -x)
    material(p, "glass", alpha_mode="blend", opacity=.16, roughness=.13)
    extrude(p, _frame_contour(2.90, 3.255, 2.12, 2.866, 3.221, 2.108, 8),
            -1.67, 1.67, t.uv("cyan", 3))
    finish(p, "A09: walk-through cyan tube, eight-facet barrel crown, open ends with white arch frames, leaf in side glazing; axis localZ")


def accent_panel(p):
    t = graphic_atlas(p, "accent_panel_atlas.png", "accent")
    material(p, "accent", roughness=.30)
    box(p, (0, 1.1, 0), (1.1, 2.2, .07), {"+z": t.uv("accent", 1),
                                                  **{f: t.uv("white", 3) for f in ("+x", "-x", "+y", "-y", "-z")}})
    finish(p, "A10: upright 1:2 cyan accent, broad diagonal white band and angular lime leaves; fitted artwork, real stock depth")


def atrium_dome(p):
    t = atlas(p)
    material(p)
    ring(p, 6.0, 5.65, 0, .20, t.uv("white", 3), 16)
    radii = (5.92, 5.35, 4.25, 2.55, .20)
    ys = tuple(.16+2.22*(1-(r/5.92)**2) for r in radii)
    # Sixteen pale radial ribs are connected swept stock, not floating sticks.
    for i in range(16):
        angle = math.tau*i/16
        radial = (math.cos(angle), math.sin(angle))
        tangent = (-radial[1], radial[0])
        sections = []
        for r, y in zip(radii, ys):
            sections.append([(radial[0]*r+tangent[0]*side*.065, y+up*.07,
                              radial[1]*r+tangent[1]*side*.065)
                             for side, up in ((-1, -1), (1, -1), (1, 1), (-1, 1))])
        loft(p, sections, t.uv("white", 3))
    cylinder(p, (0, 2.23, 0), .25, .17, t.uv("white", 3), 16)
    material(p, "glass", alpha_mode="blend", opacity=.13, roughness=.15)
    # One shallow dome shell; a real rim joins its outer and inner surfaces.
    start = len(p.mesh.indices)
    rows = [[(r*math.cos(math.tau*i/16), y, r*math.sin(math.tau*i/16))
             for i in range(16)] for r, y in zip(radii, ys)]
    inner = [[(x, y-.018, z) for x, y, z in row] for row in rows]
    uv = t.uv("cyan", 3)
    for shells in (rows, inner):
        for low, high in zip(shells, shells[1:]):
            for i in range(16):
                j = (i+1)%16
                p.mesh.quad(low[i], low[j], high[j], high[i], uv=uv, color=WHITE)
    for shell, y in ((rows, 2.38), (inner, 2.362)):
        for i in range(16):
            u0, v0, u1, v1 = uv
            p.mesh.triangle((0, y, 0), shell[-1][i], shell[-1][(i+1)%16],
                            [((u0+u1)/2, (v0+v1)/2), (u0, v1), (u1, v1)], WHITE)
    for i in range(16):
        j = (i+1)%16
        p.mesh.quad(rows[0][i], rows[0][j], inner[0][j], inner[0][i], uv=uv, color=WHITE)
    indices = p.mesh.indices[start:]
    inspect(p.mesh.positions, indices, repair=True)
    p.mesh.indices[start:] = indices
    finish(p, "A11: 12 m glazed dome with 16 connected pale radial ribs and open circular soffit; mount base over open-ceiling atrium")


def ceiling_ring(p):
    t = atlas(p)
    material(p)
    ring(p, .65, .50, .065, .065, t.uv("white", 3), 16)
    cylinder(p, (0, .055, 0), .50, .045, t.uv("white", 3), 16)
    material(p, "cyan_light", emissive=(.64, .96, 1), strength=1.55, roughness=.18)
    ring(p, .575, .49, 0, .027, t.uv("light_cyan", 3), 16)
    finish(p, "A12: recessed circular cyan ceiling ring, distinct from suspended octagonal atrium pod; emitter below base")


def _double_doors(p, opened=False):
    t = atlas(p)
    material(p)
    for x in (-1.30, 1.30):
        box(p, (x, 1.60, 0), (.20, 3.2, .34), t.uv("white", 3))
    box(p, (0, 3.10, 0), (2.42, .20, .34), t.uv("white", 3))
    material(p, "door_silver", roughness=.44, metallic=.16)
    for side in (-1, 1):
        first = len(p.mesh.positions)
        box(p, (side*.60, 1.47, 0), (1.18, 2.94, .105), t.uv("silver", 3))
        box(p, (side*.60, 1.53, .058), (.92, 2.55, .013), t.uv("silver", 5))
        if opened:
            hinge = side*1.19
            angle = side*math.radians(82)
            for i in range(first, len(p.mesh.positions)):
                x, y, z = p.mesh.positions[i]
                p.mesh.positions[i] = (hinge+(x-hinge)*math.cos(angle)+z*math.sin(angle),
                                       y, -(x-hinge)*math.sin(angle)+z*math.cos(angle))
    material(p, "door_handle", roughness=.5)
    for side in (-1, 1):
        first = len(p.mesh.positions)
        box(p, (side*.13, 1.29, .097), (.037, .29, .07), t.uv("dark", 3))
        if opened:
            hinge = side*1.19
            angle = side*math.radians(82)
            for i in range(first, len(p.mesh.positions)):
                x, y, z = p.mesh.positions[i]
                p.mesh.positions[i] = (hinge+(x-hinge)*math.cos(angle)+z*math.sin(angle),
                                       y, -(x-hinge)*math.sin(angle)+z*math.cos(angle))
    finish(p, "A12: gray terminal double doors with white jamb/header, inset leaf faces and paired dark handles" +
           ("; authored open 82 degrees, structural passage remains clear" if opened else "; closed static presentation"))


def double_doors(p):
    _double_doors(p)


def double_doors_open(p):
    _double_doors(p, True)


def _counter_outline(outer=4.4, inner=3.55):
    # Shallow crescent counter: convex player-facing front, concave staff side.
    angles = [math.radians(-30+i*5) for i in range(13)]
    return [(outer*math.sin(a), outer*math.cos(a)-3.70) for a in angles] + \
           [(inner*math.sin(a), inner*math.cos(a)-3.70) for a in reversed(angles)]


def _vertical_extrusion(p, outline, y0, y1, uv):
    first = len(p.mesh.positions)
    extrude(p, outline, y0, y1, uv)
    for i in range(first, len(p.mesh.positions)):
        x, z, y = p.mesh.positions[i]
        p.mesh.positions[i] = (x, y, z)


def reception_counter(p):
    t = atlas(p)
    material(p)
    _vertical_extrusion(p, _counter_outline(), .13, 1.03, t.uv("white", 3))
    _vertical_extrusion(p, _counter_outline(4.5, 3.49), 1.018, 1.105, t.uv("white", 3))
    material(p, "counter_aqua", roughness=.25)
    _vertical_extrusion(p, _counter_outline(4.395, 3.56), 0, .15, t.uv("aqua", 3))
    material(p, "terminal", roughness=.38)
    for x in (-1.18, 1.18):
        rounded_box(p, (x, 1.13, -.115), (.36, .05, .27), t.uv("dark", 3), .028, 2)
        box(p, (x, 1.215, -.13), (.08, .16, .08), t.uv("dark", 3))
        rounded_box(p, (x, 1.285, -.085), (.38, .19, .082), t.uv("dark", 3), .024, 2)
    material(p, "terminal_screen", roughness=.18)
    for x in (-1.18, 1.18):
        box(p, (x, 1.287, -.036), (.318, .142, .006), {"+z": t.uv("terminal", 1),
             **{f: t.uv("dark", 3) for f in ("+x", "-x", "+y", "-y", "-z")}})
    finish(p, "A13: curved white reception counter with aqua foot strip and exactly two dark countertop terminals; front +Z, standable top1.105m")


def reception_soffit(p):
    t = atlas(p)
    material(p)
    rounded_box(p, (0, .22, 0), (5.2, .24, 2.1), t.uv("white", 3), .55, 4)
    # Keep the white underside above the emitter caps: coincident y=0 faces
    # expose their competing triangle fans during the native depth pass.
    rounded_box(p, (0, .105, 0), (4.9, .17, 1.80), t.uv("white", 3), .47, 4)
    material(p, "warm_downlight", emissive=(1, .94, .77), strength=1.2, roughness=.24)
    for x in (-1.6, 0, 1.6):
        cylinder(p, (x, 0, .20), .11, .025, t.uv("warm_light", 3), 12)
    finish(p, "A13: broad rounded reception soffit with three small warm downlights; explicit map emitter taps below base")


def _banner(p, name):
    t = graphic_atlas(p, "banner_" + name + ".png", name)
    material(p)
    box(p, (0, 1.2, 0), (1.2, 2.4, .06), {"+z": t.uv(name, 1),
        **{f: t.uv("white", 3) for f in ("+x", "-x", "+y", "-y", "-z")}})
    finish(p, "A14: fitted 1:2 " + name + " nature/optimism typography with real panel depth, front +Z")


def banner_atrium(p):
    _banner(p, "atrium")


def banner_corridor(p):
    _banner(p, "corridor")


def banner_reception(p):
    _banner(p, "reception")


def city_backdrop(p):
    t = atlas(p)
    material(p, "skyline", roughness=.70)
    buildings = [(-11.0, 2.8, 1.7, .95), (-8.9, 5.0, 1.4, 1.65),
                 (-7.0, 3.8, 1.5, .65), (-4.9, 6.6, 1.4, 1.85),
                 (-2.9, 4.2, 1.65, .8), (-.4, 8.6, 1.7, 2.3),
                 (2.0, 5.7, 1.45, .55), (4.25, 7.3, 1.5, 1.75),
                 (6.6, 4.9, 1.9, 1.05), (9.0, 6.2, 1.3, .15), (11.0, 3.5, 2.0, 1.8)]
    for i, (x, h, width, z) in enumerate(buildings):
        box(p, (x, h/2, z), (width, h, .68), t.uv("sky" if i%3 else "cyan", 3))
        if i % 3 == 1:
            box(p, (x-.20, h+.12, z), (width*.65, .24, .58), t.uv("sky", 3))
    material(p, "skyline_windows", roughness=.68)
    for i, (x, h, width, z) in enumerate(buildings):
        for y in (h*.27, h*.52, h*.77):
            box(p, (x, y, z+.347), (width*.72, .065, .008), t.uv("light_cyan", 3))
    finish(p, "A15: eleven distinct blue distant city towers with varied heights/crowns and restrained window bands; nontraversable backdrop")


def green_backdrop(p):
    t = atlas(p)
    material(p, "scenery", roughness=.88)
    outline = [(-12, -1.5), (-9, -2.5), (-4, -2.25), (0, -2.45), (4, -2.10),
               (9, -2.5), (12, -1.4), (12, 2.5), (7, 2.15), (1, 2.5), (-6, 2.35), (-12, 2.5)]
    lower = [(x, 0, z) for x, z in outline]
    top = [(x*.98, .50+.27*(1+math.sin(i*1.6)), z*.94) for i, (x, z) in enumerate(outline)]
    crown = [(x*.68, 1.08+.22*math.cos(i*1.4), z*.50) for i, (x, z) in enumerate(outline)]
    loft(p, [lower, top, crown], t.uv("foliage_light", 3))
    finish(p, "A15: broad low faceted fresh-green scenic bank beyond glazing; visual boundary only, no fictional walkable floor")


PROPS = {"frutiger_aero:" + name: globals()[name] for name in SIZES}


def catalog_entries():
    """Registration proposals; caller retains ownership of the shared catalog."""
    return [dict(id="frutiger_aero:"+name,
                 display_name="Frutiger Aero "+name.replace("_", " ").title(),
                 asset_class="environment", theme="frutiger_aero", asset_type="prop",
                 source="file", model=f"environment/frutiger_aero/props/models/{name}.glb",
                 size=list(size), color="#f5f0ea", category="Frutiger Aero",
                 solid=name in ("tree_planter", "seating_pod", "display_kiosk", "accent_panel"))
            for name, size in SIZES.items()]


# Offsets removed by floor-contact/horizontal-centre normalization. Structural
# specifications below use construction coordinates and subtract these offsets.
ORIGIN_SHIFTS = {
    "wall_bay": (0, 0, -.142), "wall_bay_tall": (0, 0, -.142),
    "lime_arch": (0, 0, .0065), "tree_planter": (.0371569923, 0, .0416212190),
    "display_kiosk": (0, 0, .0085), "bubble_sculpture": (-.0525528258, 0, -.015),
    "double_doors_open": (0, 0, .5029114545), "reception_counter": (0, 0, .0612143296),
    "city_backdrop": (.075, 0, 1.2305),
}


def _facet_height(x, width, crown, jamb, segments=8):
    """Height on the actual faceted elliptical upper profile."""
    points = [(width/2*math.cos(math.pi-i*math.pi/segments),
               jamb+(crown-jamb)*math.sin(math.pi-i*math.pi/segments))
              for i in range(segments+1)]
    for (ax, ay), (bx, by) in zip(points, points[1:]):
        if ax-1e-8 <= x <= bx+1e-8:
            return ay+(by-ay)*(x-ax)/(bx-ax)
    return jamb


def structural_components(name):
    """Local collision stock, preserving open portals, seats and basin center.

    Each box is (centerX, baseY, centerZ, width, height, depth, localYawDegrees).
    Real floors remain map-owned. Architecture and scenery use solid:false;
    append these narrow carriers/structural blockers where access is intended.
    """
    name = name.removeprefix("frutiger_aero:")
    if name not in SIZES:
        raise ValueError(f"unknown Aero module {name}")
    stock = []
    if name in ("wall_bay", "wall_bay_tall"):
        h = SIZES[name][1]
        stock = [(0, .004, 0, 3.59, h-.008, .024, 0),
                 (1.68, .008, -.145, .23, h-.016, .535, 0)]
    elif name == "lime_arch":
        # Jambs plus small stock strips follow both real outer/inner crowns;
        # a full-width spring-height header would falsely close the passage.
        stock = [(side*1.535, .004, 0, .52, 1.83, .43, 0) for side in (-1, 1)]
        count = 48
        for i in range(count):
            ax, bx = -1.795+i*3.59/count, -1.795+(i+1)*3.59/count
            inner = lambda x: (_facet_height(x, 2.54, 2.85, 1.82) if abs(x)<=1.27 else 1.82)
            base = max(inner(ax), inner(bx))+.004
            top = min(_facet_height(ax, 3.6, 3.4, 1.84),
                      _facet_height(bx, 3.6, 3.4, 1.84))-.004
            if top > base:
                stock.append(((ax+bx)/2, base, 0, bx-ax, top-base, .43, 0))
    elif name == "glass_partition":
        points = [(-1.72, .32), (-.58, -.32), (.58, .32), (1.72, -.32)]
        for a, b in zip(points, points[1:]):
            for i in range(8):
                t0, t1 = i/8, (i+1)/8
                ax, az = a[0]+(b[0]-a[0])*t0, a[1]+(b[1]-a[1])*t0
                bx, bz = a[0]+(b[0]-a[0])*t1, a[1]+(b[1]-a[1])*t1
                stock.append(((ax+bx)/2, .004, (az+bz)/2, abs(bx-ax)+.014,
                              2.488, abs(bz-az)+.014, 0))
    elif name == "tree_planter":
        stock = [(0, .004, 0, 1.24, .638, 1.24, 0), (0, .65, 0, .15, 1.55, .15, 0)]
    elif name == "seating_pod":
        stock = [(0, .23, .045, 2.15, .41, .91, 0), (0, .23, -.44, 2.16, .805, .21, 0)]
        for side in (-1, 1):
            for i in range(11):
                front = -.33+(i+1)*.08
                top = 1.04 if front <= -.25 else .70 if front >= .15 else 1.04-(front+.25)/.40*.34
                stock.append((side*1.085, .23, front-.04, .21, top-.236, .08, 0))
    elif name == "display_kiosk":
        stock = [(0, .004, 0, .99, .688, .565, 0), (0, .70, -.055, .82, 1.69, .17, 0)]
    elif name == "fountain_basin":
        # Small tangential stock boxes follow the circular rim; no square
        # collider closes the fountain center or its open surrounding paths.
        for i in range(20):
            angle = math.tau*(i+.5)/20
            stock.append((1.46*math.cos(angle), .075, 1.46*math.sin(angle),
                          .42, .375, .245, math.degrees(math.pi/2-angle)))
    elif name == "glass_canopy":
        stock = [(side*1.4415, .006, 0, .013, 2.108, 3.334, 0) for side in (-1, 1)]
        # Inscribe stock in the actual 8-facet glass shell. The lower inner
        # endpoint puts a carrier below sloping glazing; use the highest
        # inner and lowest outer heights instead. Split at both profiles'
        # vertices so their extrema cannot hide inside a strip, then bound
        # width by slope/thickness: uniform 96-strip stock still cannot fit
        # the steep end facets. Keep at least 12 mm of real collision height.
        inner = lambda x: _facet_height(x, 2.866, 3.221, 2.108)
        outer = lambda x: _facet_height(x, 2.90, 3.255, 2.12)
        limit, margin, minimum_height = 1.432, .002, .012
        knots = sorted({-limit, limit, 0.,
                        *(round(width/2*math.cos(math.pi-i*math.pi/8), 12)
                          for width in (2.866, 2.90) for i in range(9)
                          if -limit < width/2*math.cos(math.pi-i*math.pi/8) < limit)})
        for a, b in zip(knots, knots[1:]):
            thickness = min(outer(a)-inner(a), outer(b)-inner(b))
            slope = max(abs(inner(b)-inner(a)), abs(outer(b)-outer(a)))/(b-a)
            maximum_width = min(.03, (thickness-2*margin-minimum_height)/slope) if slope else .03
            count = max(1, math.ceil((b-a)/maximum_width))
            for i in range(count):
                ax, bx = a+(b-a)*i/count, a+(b-a)*(i+1)/count
                base = max(inner(ax), inner(bx))+margin
                top = min(outer(ax), outer(bx))-margin
                stock.append(((ax+bx)/2, base, 0, bx-ax, top-base, 3.334, 0))
    elif name in ("double_doors", "double_doors_open"):
        stock = [(side*1.30, .004, 0, .19, 3.188, .33, 0) for side in (-1, 1)]
        stock.append((0, 3.008, 0, 2.41, .184, .33, 0))
        if name == "double_doors":
            stock += [(side*.60, .004, 0, 1.175, 2.932, .095, 0) for side in (-1, 1)]
        else:
            for side in (-1, 1):
                angle = side*math.radians(82)
                x = side*1.19-side*.59*math.cos(angle)
                z = side*.59*math.sin(angle)
                stock.append((x, .004, z, 1.175, 2.932, .095, side*82))
    elif name == "reception_counter":
        for i in range(12):
            a = math.radians(-27.5+i*5)
            stock.append((3.975*math.sin(a), .004, 3.975*math.cos(a)-3.70,
                          .36, 1.096, .825, -math.degrees(a)))
    elif name == "accent_panel":
        stock = [(0, .004, 0, 1.09, 2.188, .06, 0)]
    shift = ORIGIN_SHIFTS.get(name, (0, 0, 0))
    return dict(collision_boxes=[[x-shift[0], y-shift[1], z-shift[2], w, h, d, yaw]
                                  for x, y, z, w, h, d, yaw in stock],
                floor_support="Map owns a continuous floor; visual GLBs never supply walkable support.",
                origin_shift=list(shift))


def placed_components(name, x, z, *, base_y=0, floor_y=0, rotation_degrees=0, identity=None):
    """Map-ready visual placement and narrow existing collision-peg carriers."""
    name = name.removeprefix("frutiger_aero:")
    if not all(math.isfinite(v) for v in (x, z, base_y, floor_y, rotation_degrees)):
        raise ValueError("Aero placement coordinates must be finite")
    yaw = math.radians(rotation_degrees)
    identity = identity or "aero_"+name
    props = [dict(id=identity, model="frutiger_aero:"+name, x=x, z=z,
                  y=base_y-floor_y, rotation_degrees=rotation_degrees,
                  size=list(SIZES[name]), solid=False)]
    # Collision size is explicit and independent of the peg mesh. Opaque stock
    # hides a 1.8 mm carrier; BLEND glazing reveals it, so use an 18 micrometre
    # carrier there while keeping the same fitted physical extents.
    carrier_scale = .0003 if name == "glass_canopy" else .03
    for i, (px, py, pz, w, h, d, local_yaw) in enumerate(structural_components(name)["collision_boxes"]):
        wx = x+px*math.cos(yaw)+pz*math.sin(yaw)
        wz = z-px*math.sin(yaw)+pz*math.cos(yaw)
        props.append(dict(id=f"{identity}_solid_{i}", model="outdoor:collision_peg",
                          x=wx, z=wz, y=base_y+py-floor_y,
                          rotation_degrees=rotation_degrees+local_yaw, scale=carrier_scale,
                          size=[w/carrier_scale, h/carrier_scale, d/carrier_scale],
                          solid=True, occludes=False))
    return {"props": props}

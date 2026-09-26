"""The core pack's decorative props: plant, rug, lamp and TV.

Same contract as ``parts/utility.py`` (read that file first): the catalogue
size is authoritative, the origin is the floor-contact centre, ``+Z`` faces the
player, one 64x64/128x128 texture per prop and colours from :mod:`palette`.
The four pieces share the pack's muted, faded domestic look:

* ``core:plant`` uses a file-backed terracotta/soil/leaf/stem atlas and
  fourteen thin closed leaf shells; no alpha cut-outs;
* ``core:rug`` remains a 12-triangle slab, with fitted woven artwork;
* ``core:lamp`` uses closed base and stem components and a hollow, hemmed
  24-segment fabric shade. It emits no light;
* ``core:tv`` is the catalogue's flat 0.10 m panel: a slim housing, a four-bar
  bezel and a dark, static screen.  No video, no glow, no animation.

Wear is kept to the same faint level as the rest of the pack (a few grime
spots, dulled edges, no gloss) so no decorative prop shouts at 480x272.
"""

from __future__ import annotations

import math

import palette
from parts.refreshed import load_atlas, solid_box, solid_cylinder
from mesh import PropBuilder

# Triangle aims from the pack brief (``build.py`` enforces the hard budgets).
TARGETS = {
    "core:plant": 240,
    "core:rug": 12,
    "core:lamp": 180,
    "core:tv": 90,
}


# --------------------------------------------------------------------- paint


def _tint(color: tuple[int, int, int], lift: float = 0.55) -> tuple[int, int, int]:
    """Vertex colour for a face whose texture is painted in ``color``.

    The shader is ``texture * vertex colour * face shade``; tinting with the
    texture's own colour would multiply it into mud, so the tint is lifted
    towards the pack's light neutral (the same trick as ``parts/furniture``).
    """
    return palette.mix(color, palette.hex_to_rgb(palette.PLASTIC_WHITE), lift)


# ------------------------------------------------------------ blade helpers


def _normalize(vector: tuple[float, float, float]) -> tuple[float, float, float]:
    length = math.sqrt(vector[0] ** 2 + vector[1] ** 2 + vector[2] ** 2)
    if length < 1e-9:
        return (0.0, 0.0, 1.0)
    return (vector[0] / length, vector[1] / length, vector[2] / length)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _lerp(a, b, t: float):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def _winding(points) -> tuple[float, float, float]:
    """Newell normal of a polygon, used to keep a leaf's bright side up."""
    normal = [0.0, 0.0, 0.0]
    for index, point in enumerate(points):
        other = points[(index + 1) % len(points)]
        normal[0] += (point[1] - other[1]) * (point[2] + other[2])
        normal[1] += (point[2] - other[2]) * (point[0] + other[0])
        normal[2] += (point[0] - other[0]) * (point[1] + other[1])
    return (normal[0], normal[1], normal[2])


def _blade(mesh, points, uvs, reference, up, shade: float = 1.0) -> None:
    """Emit a leaf's upper skin; _leaf adds a separate lower skin and rim."""
    order = list(points)
    uv_order = list(uvs)
    if sum(a * b for a, b in zip(_winding(order), reference)) < 0.0:
        order.reverse()
        uv_order.reverse()
    if len(order) == 4:
        mesh.quad(*order, uv=uv_order, color=up, shade_mult=shade)
    else:
        mesh.triangle(*order, uvs=uv_order, color=up, shade_mult=shade)


def _leaf(mesh, base, tip, bend, width: float, uv, up, down, shade: float = 1.0) -> None:
    """One folded leaf with a thin closed shell instead of coplanar backfaces."""
    start = len(mesh.indices)
    azimuth = math.atan2(tip[2] - base[2], tip[0] - base[0])
    side = (-math.sin(azimuth), 0.0, math.cos(azimuth))
    mid = tuple((a + b) * 0.5 + c for a, b, c in zip(base, tip, bend))
    direction = _normalize(tuple(m - b for m, b in zip(mid, base)))
    normal = _normalize(_cross(direction, side))

    # Spine points: the blade bows out of the base-tip plane so it is not a
    # flat card, and the tip stays exactly where the caller asked for it.
    spine = [
        base,
        tuple(p + n * (width * 0.05) for p, n in zip(_lerp(base, mid, 0.62), normal)),
        tuple(p + n * (width * 0.12) for p, n in zip(mid, normal)),
        tip,
    ]
    # Widest just past the base and tapering to a short blunt tip: a broad
    # paddle reads as a paper dart from across the room, a lance does not.
    widths = (width * 0.40, width * 0.85, width * 0.62, width * 0.14)
    left = [(p[0] - side[0] * w, p[1], p[2] - side[2] * w) for p, w in zip(spine, widths)]
    right = [(p[0] + side[0] * w, p[1], p[2] + side[2] * w) for p, w in zip(spine, widths)]

    u0, v0, u1, v1 = uv
    rows = (v1, v0 + 0.62 * (v1 - v0), v0 + 0.26 * (v1 - v0), v0)
    left_tip_u = u0 + (u1 - u0) * 0.40
    right_tip_u = u0 + (u1 - u0) * 0.60
    _blade(mesh, [left[0], right[0], right[1], left[1]],
           [(u0, rows[0]), (u1, rows[0]), (u1, rows[1]), (u0, rows[1])],
           normal, up, shade)
    _blade(mesh, [left[1], right[1], right[2], left[2]],
           [(u0, rows[1]), (u1, rows[1]), (u1, rows[2]), (u0, rows[2])],
           normal, up, shade)
    _blade(mesh, [left[2], right[2], right[3], left[3]],
           [(u0, rows[2]), (u1, rows[2]), (right_tip_u, rows[3]), (left_tip_u, rows[3])],
           normal, up, shade)

    # Preserve the upper silhouette; offset the back by 0.8 mm and join only
    # boundary edges. Shared fold edges stay internal to each continuous skin.
    edges = {}
    top_indices = list(mesh.indices[start:])
    def underside(point):
        return tuple(point[i] - normal[i] * 0.0008 for i in range(3))
    for offset in range(0, len(top_indices), 3):
        ids = top_indices[offset:offset + 3]
        mesh.triangle(*(underside(mesh.positions[i]) for i in reversed(ids)),
                      uvs=[mesh.uvs[i] for i in reversed(ids)], color=down,
                      shade_mult=shade * 0.85)
        for a, b in zip(ids, ids[1:] + ids[:1]):
            key = tuple(sorted((mesh.positions[a], mesh.positions[b])))
            if key in edges:
                del edges[key]
            else:
                edges[key] = (a, b)
    for a, b in edges.values():
        pa, pb = mesh.positions[a], mesh.positions[b]
        mesh.quad(pb, pa, underside(pa), underside(pb),
                  uv=[mesh.uvs[b], mesh.uvs[a], mesh.uvs[a], mesh.uvs[b]],
                  color=down, shade_mult=shade * 0.9)


# --------------------------------------------------------------------- plant


def build_plant(p: PropBuilder) -> None:
    """Potted plant: closed pot, soil insert, stem and fourteen leaf shells."""
    tex = load_atlas(p, "plant", ("pot", "soil", "leaf", "stem"))
    pot = soil = stem = (255, 255, 255)

    # --- geometry: pot and soil ---------------------------------------------
    segments = 12
    pot_height = 0.27
    solid_cylinder(p, (0.0, 0.0, 0.0), 0.155, pot_height, segments=segments, taper=1.06, bottom=True,
               side_uv=tex.uv("pot", inset=2), cap_uv=tex.uv("pot", inset=2), color=pot)
    # Closed soil insert intersects the pot cap; no floating open disc.
    solid_cylinder(p, (0.0, pot_height - 0.003, 0.0), 0.14, 0.0045,
                   segments=12, uv=tex.uv("soil", inset=2), color=soil, shades=False)

    # --- geometry: trunk ----------------------------------------------------
    # A bare standard stem long enough to show below the lowest leaf: the
    # cheapest silhouette that reads as an indoor plant rather than a bush.
    solid_cylinder(p, (0.0, pot_height - 0.01, 0.0), 0.030, 0.52, segments=6, taper=0.72,
               side_uv=tex.uv("stem", inset=2), cap_uv=tex.uv("stem", inset=2), color=stem)

    # --- geometry: the crown of leaves --------------------------------------
    # Fourteen large blades in three tiers rather than many small ones: at
    # 480x272 the gaps between separate leaves are what makes the prop read as
    # a houseplant, so every leaf is long (0.33-0.40 m of arc), wide and
    # clearly separated by azimuth and height.  The four axis-aligned upper
    # leaves own the catalogue footprint (tips at +-0.192 m, a 0.384 m spread
    # inside the 0.40 m size) and the tallest tips stop just under 1.00 m.
    #
    # (azimuth, base height, tip radius, tip height, blade width, shade)
    # Blade widths are roughly 40 % of the leaf's length: broad enough to close
    # the crown at 480x272, narrow enough that no leaf reads as a paper dart.
    leaves = (
        # upper fan: long leaves on the top third of the stem, each tipping
        # over so its flat face still catches light from above
        (0.0, 0.68, 0.192, 0.900, 0.132, 1.02),
        (180.0, 0.67, 0.192, 0.915, 0.130, 0.97),
        (90.0, 0.70, 0.192, 0.880, 0.128, 1.00),
        (270.0, 0.69, 0.192, 0.930, 0.126, 0.94),
        (315.0, 0.74, 0.150, 0.995, 0.118, 1.05),
        (135.0, 0.73, 0.146, 0.985, 0.116, 0.92),
        # middle fan: the azimuth gaps left by the top four
        (45.0, 0.56, 0.176, 0.800, 0.121, 0.99),
        (225.0, 0.55, 0.172, 0.790, 0.120, 1.03),
        (20.0, 0.52, 0.150, 0.700, 0.114, 0.95),
        (200.0, 0.51, 0.146, 0.690, 0.113, 1.01),
        # lower pair: shorter leaves on the bare stem, heavier droop
        (160.0, 0.42, 0.142, 0.590, 0.108, 0.96),
        (340.0, 0.41, 0.138, 0.575, 0.105, 1.04),
        (110.0, 0.34, 0.130, 0.470, 0.101, 1.02),
        (290.0, 0.33, 0.126, 0.455, 0.098, 0.98),
    )
    leaf_uv = tex.uv("leaf", inset=2)
    leaf_up = (255, 255, 255)
    leaf_down = (225, 235, 220)
    for azimuth, base_y, tip_radius, tip_y, width, shade in leaves:
        angle = math.radians(azimuth)
        direction = (math.cos(angle), 0.0, math.sin(angle))
        base = (direction[0] * 0.030, base_y, direction[2] * 0.030)
        tip = (direction[0] * tip_radius, tip_y, direction[2] * tip_radius)
        # The bend lifts the blade's middle and lets the tip fall away: the
        # leaf arcs outwards instead of running straight up the stem.
        bend = (direction[0] * 0.030, 0.020 + 0.045 * max(0.0, base_y - 0.33), direction[2] * 0.030)
        _leaf(p.mesh, base, tip, bend, width, leaf_uv, leaf_up, leaf_down, shade)
    p.add_note("fourteen closed folded leaf blades in three tiers; no alpha cut-outs")


# ----------------------------------------------------------------------- rug


def build_rug(p: PropBuilder) -> None:
    """Rug: one thin box; the weave, border and wear are all painted."""
    size = p.size  # [2.0, 0.02, 1.4]
    tex = load_atlas(p, "rug", ())
    if (tex.width, tex.height) != (256, 256):
        raise ValueError("rug.png must be the delivered 256x256 woven atlas")
    # The delivered artwork's bound rug occupies the upper 169 rows of the
    # native 256 px atlas; the binding/backing occupies the bottom 84 rows
    # (172-255), with a narrow gutter between the two. The historical 128 px
    # atlas used 84/42 rows; the native atlas is the size that ships.
    tex.region("top", (0, 0, 256, 169))
    tex.region("edge", (0, 172, 256, 84))
    field = border = (255, 255, 255)

    # --- geometry -----------------------------------------------------------
    face = field
    solid_box(p,
        (0.0, size[1] * 0.5, 0.0),
        (size[0], size[1], size[2]),
        uv={"+y": tex.uv("top", inset=2), "-y": tex.uv("edge", inset=2), "+z": tex.uv("edge", inset=2),
            "-z": tex.uv("edge", inset=2), "+x": tex.uv("edge", inset=2), "-x": tex.uv("edge", inset=2)},
        color=border,
        colors={"+y": face},
    )
    p.add_note("woven field, border and wear are paint; the mesh is one thin slab")


# ---------------------------------------------------------------------- lamp


def build_lamp(p: PropBuilder) -> None:
    """Floor lamp: weighted base, thin stem, tapered shade.  It does not glow."""
    size = p.size  # [0.35, 1.5, 0.35]
    tex = load_atlas(p, "lamp", ("base", "stem", "shade", "inner"))
    metal = metal_dark = shade = (255, 255, 255)

    # --- geometry -----------------------------------------------------------
    segments = 24
    shade_bottom = 1.18
    shade_radius = size[0] * 0.5  # the shade's bottom rim owns the footprint
    shade_taper = 0.57
    shade_top = size[1]
    solid_cylinder(p, (0.0, 0.0, 0.0), 0.160, 0.028, segments=segments, bottom=True,
               side_uv=tex.uv("base", inset=2), cap_uv=tex.uv("base", inset=2), color=metal)
    solid_cylinder(p, (0.0, 0.026, 0.0), 0.092, 0.034, segments=segments, taper=0.70,
               side_uv=tex.uv("base", inset=2), cap_uv=tex.uv("base", inset=2), color=metal)
    solid_cylinder(p, (0.0, 0.055, 0.0), 0.015, 1.060, segments=8,
               side_uv=tex.uv("stem", inset=2), cap_uv=tex.uv("stem", inset=2), color=metal)
    solid_cylinder(p, (0.0, 1.113, 0.0), 0.028, 0.067, segments=8,
               side_uv=tex.uv("stem", inset=2), cap_uv=tex.uv("inner", inset=2), color=metal_dark)
    # A hollow shell: outer cloth, inner lining and annular hems. Both
    # openings remain open; the solid cap in the old shade hid its interior.
    outer_bottom, outer_top = shade_radius, shade_radius * shade_taper
    inner_bottom, inner_top = outer_bottom - 0.004, outer_top - 0.004
    def ring(radius, y, angle):
        return (math.cos(angle) * radius, y, math.sin(angle) * radius)
    for index in range(segments):
        a0, a1 = math.tau * index / segments, math.tau * (index + 1) / segments
        ob0, ob1 = ring(outer_bottom, shade_bottom, a0), ring(outer_bottom, shade_bottom, a1)
        ot0, ot1 = ring(outer_top, shade_top, a0), ring(outer_top, shade_top, a1)
        ib0, ib1 = ring(inner_bottom, shade_bottom, a0), ring(inner_bottom, shade_bottom, a1)
        it0, it1 = ring(inner_top, shade_top, a0), ring(inner_top, shade_top, a1)
        u0, v0, u1, v1 = tex.uv("shade", inset=2)
        ua = u0 + (u1 - u0) * index / segments
        ub = u0 + (u1 - u0) * (index + 1) / segments
        p.mesh.quad(ob1, ob0, ot0, ot1,
                    uv=[(ub, v1), (ua, v1), (ua, v0), (ub, v0)], color=shade,
                    shade_mult=0.82 + 0.18 * (0.5 + 0.5 * math.cos(a0 - 0.9)))
        p.mesh.quad(ib0, ib1, it1, it0, uv=tex.uv("inner", inset=2), color=(210, 210, 210))
        p.mesh.quad(ot1, ot0, it0, it1, uv=tex.uv("shade", inset=2), color=shade)
        p.mesh.quad(ob0, ob1, ib1, ib0, uv=tex.uv("shade", inset=2), color=(220, 220, 220))
    p.add_note("shade is fabric and geometry only; the lamp emits no light")


# ------------------------------------------------------------------------ tv


def build_tv(p: PropBuilder) -> None:
    """Television: slim housing, four-bar bezel and a dark, static screen.

    The catalogue depth (0.10 m) makes this a flat panel, so the housing is a
    slab, the bezel is four proud bars and the screen is one recessed quad.  It
    is a screen, not a light: no glow, no animation.
    """
    size = p.size  # [1.1, 0.7, 0.1]
    tex = p.set_texture(64, seed=107)
    tex.auto("screen", "bezel", "panel", "body")

    case = palette.mix(palette.hex_to_rgb(palette.METAL_GREY), palette.hex_to_rgb(palette.PLASTIC_DARK), 0.42)
    case_light = palette.shade(case, 1.08)
    case_dark = palette.shade(case, 0.66)
    # Painted a touch lighter than the palette's "screen dark": the shader
    # multiplies texture and vertex colour, and a screen painted at the dark
    # end of the palette renders as a black hole rather than as static.
    screen = palette.shade(palette.mix(palette.hex_to_rgb(palette.SCREEN_DARK),
                                       palette.hex_to_rgb(palette.GLASS_TINT), 0.26), 1.05)
    dark = palette.hex_to_rgb(palette.ELECTRONICS_DARK)

    # --- texture: the screen (dark, switched off, faintly reflective) --------
    # A switched-off LCD is a dark grey sheet with one diagonal sheen, not a
    # striped pattern: a couple of very faint scanline bands are all the detail
    # it takes to read as a screen at 480x272.
    tex.gradient("screen", palette.shade(screen, 1.30), palette.shade(screen, 0.74), jitter=3, seed=3)
    tex.band("screen", palette.shade(screen, 1.42), 0.06, 0.13, alpha=26)
    tex.bar("screen", palette.shade(screen, 1.18), (0.60, 0.0, 0.66, 1.0), alpha=22)
    for row in range(0, 32, 6):
        tex.band("screen", palette.shade(screen, 0.90), row / 32.0, (row + 1) / 32.0, alpha=40)
    tex.spots("screen", palette.hex_to_rgb(palette.GRIME), count=3, seed=5, radius=1, alpha=22)
    tex.border("screen", palette.shade(screen, 0.58), width=1, alpha=150)

    # --- texture: the plain bezel bars --------------------------------------
    tex.fill("bezel", case, jitter=7, seed=11)
    tex.noise("bezel", amount=4, freq=3, seed=12)
    tex.grain("bezel", case_dark, seed=13, density=0.30, alpha=34)
    tex.grain("bezel", case_light, seed=14, density=0.24, alpha=24)
    tex.bar("bezel", case_light, (0.0, 0.0, 1.0, 0.06), alpha=90)
    tex.spots("bezel", palette.hex_to_rgb(palette.GRIME), count=2, seed=15, radius=2, alpha=22)

    # --- texture: the control bar under the screen --------------------------
    tex.fill("panel", case, jitter=7, seed=21)
    tex.grain("panel", case_dark, seed=22, density=0.30, alpha=34)
    tex.bar("panel", case_dark, (0.30, 0.22, 0.34, 0.78), alpha=160)
    tex.dots("panel", case_light, [(0.36, 0.50)], radius=1, alpha=220)
    for index in range(3):
        fx = 0.62 + index * 0.055
        tex.bar("panel", case_light, (fx, 0.30, fx + 0.028, 0.70), alpha=200)
        tex.bar("panel", case_dark, (fx + 0.028, 0.30, fx + 0.042, 0.70), alpha=200)
    tex.scribble("panel", palette.shade(case_dark, 0.85), (0.05, 0.30, 0.26, 0.70), seed=23,
                 text_blocks=1, alpha=120)

    # --- texture: housing, flanks and back (darker, vented) ------------------
    tex.fill("body", palette.shade(case_dark, 0.80), jitter=6, seed=31)
    tex.grain("body", palette.shade(case_dark, 0.6), seed=32, density=0.32, alpha=40)
    for index in range(6):
        py = 0.30 + index * 0.07
        tex.bar("body", palette.shade(case_dark, 0.55), (0.10, py, 0.90, py + 0.035), alpha=140)
    tex.spots("body", palette.hex_to_rgb(palette.GRIME), count=3, seed=33, radius=2, alpha=24)
    tex.border("body", palette.shade(case_dark, 0.6), width=1, alpha=90)

    # --- geometry -----------------------------------------------------------
    # A thin bezel and housing on a small pedestal stand. The panel itself is
    # 1.10 x 0.61 m (a 16:9 sheet of glass), lifted 9 cm by the stand so the
    # prop reads as a television rather than a picture frame laid against the
    # floor. The panel's top edge is the catalogue height.
    half_d = size[2] * 0.5
    stand_top = 0.09
    bezel_d = 0.022
    box_d = size[2] - bezel_d     # housing fills the depth behind the bezel
    bezel_z = half_d - bezel_d * 0.5
    side_bar, top_bar, chin = 0.035, 0.030, 0.065
    panel_top = size[1]
    panel_bottom = stand_top
    screen_bottom = panel_bottom + chin
    screen_top = panel_top - top_bar
    frame = _tint(case, 0.45)
    bezel_uv = {"+z": tex.uv("bezel"), "-z": None, "+y": tex.uv("bezel"), "-y": None,
                "+x": tex.uv("bezel"), "-x": tex.uv("bezel")}
    # Stand: a low plinth and a short neck, both inside the 0.10 m depth.
    p.box((0.0, 0.015, -0.005), (0.42, 0.03, size[2] - 0.02), uv=tex.uv("body"),
          color=_tint(dark, 0.35))
    p.box((0.0, 0.06, -0.005), (0.14, 0.06, 0.06), uv=tex.uv("body"),
          color=_tint(case_dark, 0.5))
    # Housing behind the bezel, with the vented back on its rear face.
    p.box((0.0, (panel_bottom + panel_top) * 0.5, half_d - bezel_d - box_d * 0.5),
          (size[0] - 0.04, panel_top - panel_bottom - 0.02, box_d),
          uv={"+z": tex.uv("body"), "-z": tex.uv("body"), "+y": tex.uv("body"),
              "-y": None, "+x": tex.uv("body"), "-x": tex.uv("body")},
          color=_tint(case_dark, 0.40))
    # One box per bezel bar: the side bars own the panel's height, the top bar
    # and the deeper chin own its width.
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - side_bar * 0.5), (panel_bottom + panel_top) * 0.5, bezel_z),
              (side_bar, panel_top - panel_bottom, bezel_d), uv=bezel_uv, color=frame)
    p.box((0.0, panel_top - top_bar * 0.5, bezel_z), (size[0] - side_bar * 2, top_bar, bezel_d),
          uv=bezel_uv, color=frame)
    p.box((0.0, panel_bottom + chin * 0.5, bezel_z), (size[0] - side_bar * 2, chin, bezel_d),
          uv={"+z": tex.uv("panel"), "-z": None, "+y": tex.uv("bezel"), "-y": None,
              "+x": tex.uv("bezel"), "-x": tex.uv("bezel")},
          color=frame)
    # Screen: set 1 cm back from the bezel's front face, so the frame casts a
    # real recess around the glass instead of sitting proud of it.
    p.plane((0.0, (screen_bottom + screen_top) * 0.5, bezel_z + 0.022),
            (size[0] - side_bar * 2 - 0.01, screen_top - screen_bottom, 0.0), normal="z",
            uv=tex.uv("screen"), color=_tint(screen, 0.45))
    p.add_note("flat panel on a pedestal stand; bezel bars carry the size, one recessed screen quad")


PROPS = {
    "core:plant": build_plant,
    "core:rug": build_rug,
    "core:lamp": build_lamp,
    "core:tv": build_tv,
}

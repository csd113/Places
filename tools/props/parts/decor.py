"""The core pack's decorative props: plant, rug, lamp and TV.

Same contract as ``parts/utility.py`` (read that file first): the catalogue
size is authoritative, the origin is the floor-contact centre, ``+Z`` faces the
player, one 64x64/128x128 texture per prop and colours from :mod:`palette`.
The four pieces share the pack's muted, faded domestic look:

* ``core:plant`` spends the pack's one justified 128x128 texture because it has
  no other colour variation to carry it: the pot, soil, stem and leaf gradient
  each get a 64x64 cell.  Its leaves are opaque folded blades -- no alpha
  cut-outs, no leaf cards, and the widest leaf tips define the catalogue
  footprint while the pot stays comfortably inside it;
* ``core:rug`` is a 12-triangle slab of floor: weave, border and wear are
  painted, nothing is modelled;
* ``core:lamp`` is a floor lamp built from 12- and 6-segment cylinders.  It
  emits no light: the shade is simply a cream taper, and its inner surface is a
  dark reversed cone so the open end reads as an opening;
* ``core:tv`` is the catalogue's flat 0.10 m panel: a slim housing, a four-bar
  bezel and a dark, static screen.  No video, no glow, no animation.

Wear is kept to the same faint level as the rest of the pack (a few grime
spots, dulled edges, no gloss) so no decorative prop shouts at 480x272.
"""

from __future__ import annotations

import math

import palette
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


def _blade(mesh, points, uvs, reference, up, down, shade: float = 1.0, back_shade: float = 0.85) -> None:
    """Emits one leaf face twice: the outward surface and its shaded back.

    Props are drawn without alpha, so a leaf cannot be a cut-out card.  Each
    face is therefore real geometry, and the back gets its own (darker) copy so
    the plant reads from any camera angle.  Which copy is "outward" is decided
    against the leaf's own plane normal, not the world up: a steep leaf's
    normal is nearly horizontal, and a world-up test would stripe the blade
    with alternating light and dark facets.
    """
    order = list(points)
    uv_order = list(uvs)
    if sum(a * b for a, b in zip(_winding(order), reference)) < 0.0:
        order.reverse()
        uv_order.reverse()
    back = list(reversed(order))
    back_uvs = list(reversed(uv_order))
    if len(order) == 4:
        mesh.quad(*order, uv=uv_order, color=up, shade_mult=shade)
        mesh.quad(*back, uv=back_uvs, color=down, shade_mult=shade * back_shade)
    else:
        mesh.triangle(*order, uvs=uv_order, color=up, shade_mult=shade)
        mesh.triangle(*back, uvs=back_uvs, color=down, shade_mult=shade * back_shade)


def _leaf(mesh, base, tip, bend, width: float, uv, up, down, shade: float = 1.0) -> None:
    """One opaque leaf: two folded quads and a tip triangle, seen from both sides."""
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
    # Widest at 38 % of the blade and ending in a short blunt edge rather than a
    # spike: a triangle tip is what makes a leaf read as a paper dart.
    widths = (width * 0.34, width * 1.0, width * 0.70, width * 0.16)
    left = [(p[0] - side[0] * w, p[1], p[2] - side[2] * w) for p, w in zip(spine, widths)]
    right = [(p[0] + side[0] * w, p[1], p[2] + side[2] * w) for p, w in zip(spine, widths)]

    u0, v0, u1, v1 = uv
    rows = (v1, v0 + 0.62 * (v1 - v0), v0 + 0.26 * (v1 - v0), v0)
    left_tip_u = u0 + (u1 - u0) * 0.40
    right_tip_u = u0 + (u1 - u0) * 0.60
    _blade(mesh, [left[0], right[0], right[1], left[1]],
           [(u0, rows[0]), (u1, rows[0]), (u1, rows[1]), (u0, rows[1])],
           normal, up, down, shade)
    _blade(mesh, [left[1], right[1], right[2], left[2]],
           [(u0, rows[1]), (u1, rows[1]), (u1, rows[2]), (u0, rows[2])],
           normal, up, down, shade)
    _blade(mesh, [left[2], right[2], right[3], left[3]],
           [(u0, rows[2]), (u1, rows[2]), (right_tip_u, rows[3]), (left_tip_u, rows[3])],
           normal, up, down, shade)


# --------------------------------------------------------------------- plant


def build_plant(p: PropBuilder) -> None:
    """Potted plant: 12-segment pot, soil disc, bare stem, two tiers of leaves."""
    tex = p.set_texture(128, seed=71)
    tex.auto("pot", "soil", "leaf", "stem")

    pot = palette.mix(palette.hex_to_rgb(palette.CARDBOARD_DARK), palette.hex_to_rgb(palette.RUST), 0.55)
    soil = palette.mix(palette.hex_to_rgb(palette.WOOD_DARK), palette.hex_to_rgb(palette.GRIME), 0.40)
    leaf_base = palette.mix(palette.hex_to_rgb(palette.FOLIAGE_DARK), palette.hex_to_rgb(palette.FOLIAGE_GREEN), 0.70)
    leaf_tip = palette.shade(palette.hex_to_rgb(palette.FOLIAGE_LIGHT), 1.14)
    stem = palette.hex_to_rgb(palette.STEM_GREEN)

    # --- texture: pot -------------------------------------------------------
    tex.fill("pot", pot, jitter=8, seed=3)
    tex.noise("pot", amount=5, freq=3, seed=4)
    tex.grain("pot", palette.hex_to_rgb(palette.CARDBOARD_DARK), seed=5, density=0.30, alpha=40)
    tex.band("pot", palette.shade(pot, 1.18), 0.02, 0.11, alpha=70)
    tex.band("pot", palette.shade(pot, 0.78), 0.90, 1.0, alpha=80)
    tex.streaks("pot", palette.hex_to_rgb(palette.GRIME), count=3, seed=6, alpha=22)
    tex.spots("pot", palette.hex_to_rgb(palette.GRIME), count=4, seed=7, radius=2, alpha=28)
    tex.border("pot", palette.shade(pot, 0.70), width=1, alpha=70)

    # --- texture: soil (damp, a little litter) ------------------------------
    tex.fill("soil", soil, jitter=7, seed=11)
    tex.spots("soil", palette.shade(soil, 1.55), count=5, seed=12, radius=1, alpha=55)
    tex.spots("soil", palette.shade(soil, 0.55), count=5, seed=13, radius=2, alpha=65)
    tex.spots("soil", palette.mix(palette.hex_to_rgb(palette.CARDBOARD), soil, 0.5), count=3, seed=14, radius=1, alpha=60)

    # --- texture: leaf (base dark at the region's bottom, tip light on top) --
    # Kept deliberately quiet: a smooth gradient, a central rib and three faint
    # bands.  Busier paint turns the crown into noise at 480x272.
    tex.gradient("leaf", leaf_tip, leaf_base, jitter=4, seed=21)
    tex.bar("leaf", palette.shade(leaf_tip, 1.06), (0.47, 0.0, 0.53, 1.0), alpha=30)
    for index in range(3):
        v = 0.24 + index * 0.22
        tex.band("leaf", palette.shade(leaf_base, 0.90), v, v + 0.03, alpha=28)
    tex.spots("leaf", palette.hex_to_rgb(palette.RUST), count=2, seed=23, radius=1, alpha=20)
    tex.border("leaf", palette.shade(leaf_base, 0.90), width=1, alpha=20)

    # --- texture: stem ------------------------------------------------------
    tex.fill("stem", stem, jitter=6, seed=31)
    tex.grain("stem", palette.shade(stem, 0.74), seed=32, density=0.42, alpha=48)
    tex.border("stem", palette.shade(stem, 0.80), width=1, alpha=60)

    # --- geometry: pot and soil ---------------------------------------------
    segments = 12
    pot_height = 0.27
    p.cylinder((0.0, 0.0, 0.0), 0.155, pot_height, segments=segments, taper=1.06, bottom=True,
               side_uv=tex.uv("pot"), cap_uv=tex.uv("pot"), color=_tint(pot, 0.40))
    # The cylinder caps the pot; the soil disc floats just above the cap and
    # inside the rim, so the pot reads as having a wall thickness.
    top = pot_height + 0.0015
    soil_radius = 0.14
    u0, v0, u1, v1 = tex.uv("soil")
    cu, cv = (u0 + u1) * 0.5, (v0 + v1) * 0.5
    ru, rv = (u1 - u0) * 0.5, (v1 - v0) * 0.5
    soil_color = _tint(soil, 0.35)
    for index in range(segments):
        a0 = math.tau * index / segments
        a1 = math.tau * (index + 1) / segments
        p.mesh.triangle(
            (0.0, top, 0.0),
            (math.cos(a1) * soil_radius, top, math.sin(a1) * soil_radius),
            (math.cos(a0) * soil_radius, top, math.sin(a0) * soil_radius),
            [(cu, cv),
             (cu + ru * math.cos(a1), cv - rv * math.sin(a1)),
             (cu + ru * math.cos(a0), cv - rv * math.sin(a0))],
            soil_color,
            shade_mult=1.0,
        )

    # --- geometry: trunk ----------------------------------------------------
    # A bare standard stem long enough to show below the lowest leaf: the
    # cheapest silhouette that reads as an indoor plant rather than a bush.
    p.cylinder((0.0, pot_height - 0.01, 0.0), 0.030, 0.52, segments=6, taper=0.72,
               side_uv=tex.uv("stem"), cap_uv=tex.uv("stem"), color=_tint(stem, 0.35))

    # --- geometry: the crown of leaves --------------------------------------
    # Twelve large blades in three tiers rather than many small ones: at
    # 480x272 the gaps between separate leaves are what makes the prop read as
    # a houseplant, so every leaf is long (0.33-0.40 m of arc), wide and
    # clearly separated by azimuth and height.  The four axis-aligned upper
    # leaves own the catalogue footprint (tips at +-0.192 m, a 0.384 m spread
    # inside the 0.40 m size) and the tallest tips stop just under 1.00 m.
    #
    # (azimuth, base height, tip radius, tip height, blade width, shade)
    leaves = (
        # upper fan: broad leaves on the top third of the stem, each tipping
        # over so its flat face still catches light from above
        (0.0, 0.68, 0.192, 0.900, 0.186, 1.02),
        (180.0, 0.67, 0.192, 0.915, 0.184, 0.97),
        (90.0, 0.70, 0.192, 0.880, 0.180, 1.00),
        (270.0, 0.69, 0.192, 0.930, 0.176, 0.94),
        (315.0, 0.74, 0.150, 0.995, 0.166, 1.05),
        (135.0, 0.73, 0.146, 0.985, 0.162, 0.92),
        # middle fan: the azimuth gaps left by the top four
        (45.0, 0.56, 0.176, 0.800, 0.170, 0.99),
        (225.0, 0.55, 0.172, 0.790, 0.168, 1.03),
        (20.0, 0.52, 0.150, 0.700, 0.160, 0.95),
        (200.0, 0.51, 0.146, 0.690, 0.158, 1.01),
        # lower pair: short leaves on the bare stem, heavier droop
        (160.0, 0.42, 0.142, 0.590, 0.150, 0.96),
        (340.0, 0.41, 0.138, 0.575, 0.146, 1.04),
        (110.0, 0.34, 0.130, 0.470, 0.140, 1.02),
        (290.0, 0.33, 0.126, 0.455, 0.136, 0.98),
    )
    leaf_uv = tex.uv("leaf")
    leaf_up = palette.mix(leaf_tip, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.70)
    leaf_down = palette.mix(_tint(leaf_base, 0.35), palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.20)
    for azimuth, base_y, tip_radius, tip_y, width, shade in leaves:
        angle = math.radians(azimuth)
        direction = (math.cos(angle), 0.0, math.sin(angle))
        base = (direction[0] * 0.030, base_y, direction[2] * 0.030)
        tip = (direction[0] * tip_radius, tip_y, direction[2] * tip_radius)
        # The bend lifts the blade's middle and lets the tip fall away: the
        # leaf arcs outwards instead of running straight up the stem.
        bend = (direction[0] * 0.030, 0.035, direction[2] * 0.030)
        _leaf(p.mesh, base, tip, bend, width, leaf_uv, leaf_up, leaf_down, shade)
    p.add_note("twelve opaque folded leaf blades in three tiers; no alpha cut-outs")


# ----------------------------------------------------------------------- rug


def build_rug(p: PropBuilder) -> None:
    """Rug: one thin box; the weave, border and wear are all painted."""
    size = p.size  # [2.0, 0.02, 1.4]
    tex = p.set_texture(64, seed=83)
    # The top face covers 2.0 x 1.4 m, so the weave region gets 48 of the 64
    # rows: close to square texels in world space, with 16 rows for the edge.
    tex.region("top", (0, 0, 64, 48))
    tex.region("edge", (0, 48, 64, 16))

    field = palette.shade(
        palette.mix(palette.hex_to_rgb(palette.RUG_BROWN), palette.hex_to_rgb(palette.RUG_RED), 0.38), 1.10
    )
    border = palette.shade(field, 0.62)
    accent = palette.mix(palette.hex_to_rgb(palette.RUG_TEAL), border, 0.45)
    pale = palette.shade(field, 1.22)

    # --- texture: woven face ------------------------------------------------
    tex.fill("top", field, jitter=8, seed=3)
    tex.noise("top", amount=5, freq=3, seed=4)
    # Weave: horizontal dashes plus a few vertical ones, kept faint.
    tex.grain("top", border, seed=5, density=0.42, alpha=40)
    tex.grain("top", pale, seed=6, density=0.30, alpha=26)
    tex.streaks("top", border, count=22, seed=7, alpha=26, direction="v")
    # Border: 0.12 m of dark band on all four sides (the region is not square
    # in world space, so the side bands are fractionally narrower).
    tex.bar("top", border, (0.0, 0.0, 0.06, 1.0))
    tex.bar("top", border, (0.94, 0.0, 1.0, 1.0))
    tex.bar("top", border, (0.0, 0.0, 1.0, 0.086))
    tex.bar("top", border, (0.0, 0.914, 1.0, 1.0))
    tex.bar("top", accent, (0.088, 0.126, 0.098, 0.874))
    tex.bar("top", accent, (0.902, 0.126, 0.912, 0.874))
    tex.bar("top", accent, (0.088, 0.126, 0.912, 0.143))
    tex.bar("top", accent, (0.088, 0.857, 0.912, 0.874))
    # A faded inner motif: one square outline, off-centre by a mending patch.
    tex.bar("top", palette.shade(field, 1.10), (0.31, 0.36, 0.69, 0.373))
    tex.bar("top", palette.shade(field, 1.10), (0.31, 0.63, 0.69, 0.643))
    tex.bar("top", palette.shade(field, 1.10), (0.31, 0.373, 0.323, 0.63))
    tex.bar("top", palette.shade(field, 1.10), (0.677, 0.373, 0.69, 0.63))
    # Wear: grime, two worn-thin patches and a scuffed corner.
    tex.spots("top", palette.hex_to_rgb(palette.GRIME), count=6, seed=8, radius=3, alpha=26)
    tex.spots("top", pale, count=3, seed=9, radius=3, alpha=34)
    tex.spots("top", pale, count=2, seed=10, radius=4, alpha=40, sub=(0.62, 0.60, 1.0, 1.0))
    tex.border("top", palette.shade(border, 0.85), width=2, alpha=90)

    # --- texture: the 2 cm edge and underside -------------------------------
    tex.fill("edge", border, jitter=6, seed=13)
    tex.grain("edge", palette.shade(border, 0.7), seed=14, density=0.45, alpha=50)
    tex.spots("edge", palette.hex_to_rgb(palette.GRIME), count=3, seed=15, radius=2, alpha=30)

    # --- geometry -----------------------------------------------------------
    face = _tint(field, 0.68)
    p.box(
        (0.0, size[1] * 0.5, 0.0),
        (size[0], size[1], size[2]),
        uv={"+y": tex.uv("top"), "-y": tex.uv("edge"), "+z": tex.uv("edge"),
            "-z": tex.uv("edge"), "+x": tex.uv("edge"), "-x": tex.uv("edge")},
        color=_tint(border, 0.45),
        colors={"+y": face},
    )
    p.add_note("woven field, border and wear are paint; the mesh is one thin slab")


# ---------------------------------------------------------------------- lamp


def build_lamp(p: PropBuilder) -> None:
    """Floor lamp: weighted base, thin stem, tapered shade.  It does not glow."""
    size = p.size  # [0.35, 1.5, 0.35]
    tex = p.set_texture(64, seed=101)
    tex.auto("base", "stem", "shade", "inner")

    metal = palette.mix(palette.hex_to_rgb(palette.METAL_DARK), palette.hex_to_rgb(palette.WOOD_WARM), 0.30)
    metal_dark = palette.shade(metal, 0.72)
    shade = palette.mix(palette.hex_to_rgb(palette.PLASTIC_CREAM), palette.hex_to_rgb(palette.FABRIC_BEIGE), 0.35)
    inner = palette.mix(palette.hex_to_rgb(palette.METAL_DARK), palette.hex_to_rgb(palette.GRIME), 0.45)

    # --- texture: base (cast metal, scuffed by feet) -------------------------
    tex.fill("base", metal, jitter=8, seed=3)
    tex.noise("base", amount=5, freq=3, seed=4)
    tex.grain("base", metal_dark, seed=5, density=0.35, alpha=45)
    tex.streaks("base", palette.hex_to_rgb(palette.GRIME), count=3, seed=6, alpha=22)
    tex.spots("base", palette.hex_to_rgb(palette.RUST), count=3, seed=7, radius=2, alpha=30)
    tex.border("base", metal_dark, width=1, alpha=80)

    # --- texture: stem and socket -------------------------------------------
    tex.fill("stem", palette.shade(metal, 1.05), jitter=6, seed=11)
    tex.grain("stem", metal_dark, seed=12, density=0.40, alpha=50)
    tex.band("stem", palette.shade(metal, 1.25), 0.06, 0.14, alpha=90)
    tex.spots("stem", palette.hex_to_rgb(palette.RUST), count=2, seed=13, radius=1, alpha=26)

    # --- texture: shade (faded fabric, faint weave and hems) -----------------
    tex.fill("shade", shade, jitter=6, seed=21)
    tex.noise("shade", amount=4, freq=4, seed=22)
    tex.grain("shade", palette.shade(shade, 0.86), seed=23, density=0.34, alpha=32)
    tex.grain("shade", palette.shade(shade, 1.10), seed=24, density=0.26, alpha=24)
    tex.band("shade", palette.shade(shade, 0.80), 0.0, 0.05, alpha=70)
    tex.band("shade", palette.shade(shade, 0.84), 0.95, 1.0, alpha=80)
    tex.spots("shade", palette.hex_to_rgb(palette.GRIME), count=3, seed=25, radius=2, alpha=20)
    tex.border("shade", palette.shade(shade, 0.82), width=1, alpha=50)

    # --- texture: shade interior (dark, never lit) ---------------------------
    tex.fill("inner", inner, jitter=5, seed=31)
    tex.grain("inner", palette.shade(inner, 0.7), seed=32, density=0.35, alpha=40)
    tex.spots("inner", palette.shade(inner, 1.4), count=2, seed=33, radius=2, alpha=30)

    # --- geometry -----------------------------------------------------------
    segments = 12
    shade_bottom = 1.18
    shade_radius = size[0] * 0.5  # the shade's bottom rim owns the footprint
    shade_taper = 0.57
    shade_top = size[1]
    p.cylinder((0.0, 0.0, 0.0), 0.160, 0.028, segments=segments, bottom=True,
               side_uv=tex.uv("base"), cap_uv=tex.uv("base"), color=_tint(metal, 0.42))
    p.cylinder((0.0, 0.028, 0.0), 0.092, 0.032, segments=segments, taper=0.70,
               side_uv=tex.uv("base"), cap_uv=tex.uv("base"), color=_tint(metal, 0.50))
    p.cylinder((0.0, 0.055, 0.0), 0.015, 1.060, segments=6,
               side_uv=tex.uv("stem"), cap_uv=tex.uv("stem"), color=_tint(metal, 0.55))
    p.cylinder((0.0, 1.115, 0.0), 0.028, 0.065, segments=6,
               side_uv=tex.uv("stem"), cap_uv=tex.uv("inner"), color=_tint(metal_dark, 0.40))
    p.cylinder((0.0, shade_bottom, 0.0), shade_radius, shade_top - shade_bottom,
               segments=segments, taper=shade_taper, side_uv=tex.uv("shade"),
               cap_uv=tex.uv("inner"), color=_tint(shade, 0.42))
    # The open end: a reversed cone just inside the outer shade, so looking up
    # into the lamp shows a dark interior rather than a hole.
    inner_radius = shade_radius - 0.004
    inner_top = shade_radius * shade_taper - 0.004
    for index in range(segments):
        a0 = math.tau * index / segments
        a1 = math.tau * (index + 1) / segments
        bottom0 = (math.cos(a0) * inner_radius, shade_bottom + 0.002, math.sin(a0) * inner_radius)
        bottom1 = (math.cos(a1) * inner_radius, shade_bottom + 0.002, math.sin(a1) * inner_radius)
        top1 = (math.cos(a1) * inner_top, shade_top - 0.002, math.sin(a1) * inner_top)
        top0 = (math.cos(a0) * inner_top, shade_top - 0.002, math.sin(a0) * inner_top)
        p.mesh.quad(bottom1, bottom0, top0, top1, uv=tex.uv("inner"),
                    color=_tint(inner, 0.30), shade_mult=0.62 + 0.20 * (0.5 + 0.5 * math.cos(a0 - 0.9)))
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

    # --- texture: the screen (dark, static, slightly reflective) -------------
    tex.gradient("screen", palette.shade(screen, 1.22), palette.shade(screen, 0.80), jitter=4, seed=3)
    for row in range(0, 32, 2):
        tex.band("screen", palette.shade(screen, 0.74), row / 32.0, (row + 1) / 32.0, alpha=90)
    tex.streaks("screen", palette.shade(screen, 1.45), count=2, seed=4, alpha=26, direction="v")
    tex.spots("screen", palette.hex_to_rgb(palette.GRIME), count=3, seed=5, radius=1, alpha=26)
    tex.border("screen", palette.shade(screen, 0.62), width=1, alpha=130)

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
    # The four bezel bars carry the full catalogue width and height; the
    # housing sits behind them and the feet lift the panel 2 cm off the floor.
    bar_depth = 0.020
    bar_z = size[2] * 0.5 - bar_depth * 0.5
    bar = 0.05
    frame = _tint(case, 0.45)
    housing_depth = size[2] - bar_depth
    housing_front = size[2] * 0.5 - bar_depth
    p.box(
        (0.0, 0.35, housing_front - housing_depth * 0.5),
        (size[0] - 0.04, size[1] - 0.04, housing_depth),
        uv={"+z": tex.uv("screen"), "-z": tex.uv("body"), "+y": tex.uv("body"),
            "-y": None, "+x": tex.uv("body"), "-x": tex.uv("body")},
        color=_tint(case_dark, 0.40),
    )
    # One box per bezel bar: side bars own the height, top and bottom the width.
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - bar * 0.5), 0.35, bar_z), (bar, size[1], bar_depth),
              uv={"+z": tex.uv("bezel"), "-z": tex.uv("body"), "+y": tex.uv("bezel"),
                  "-y": None, "+x": tex.uv("bezel"), "-x": tex.uv("bezel")},
              color=frame)
    p.box((0.0, size[1] - bar * 0.5, bar_z), (size[0] - bar * 2, bar, bar_depth),
          uv={"+z": tex.uv("bezel"), "-z": tex.uv("body"), "+y": tex.uv("bezel"),
              "-y": None, "+x": tex.uv("bezel"), "-x": tex.uv("bezel")},
          color=frame)
    p.box((0.0, bar * 0.5 + 0.02, bar_z), (size[0] - bar * 2, bar, bar_depth),
          uv={"+z": tex.uv("panel"), "-z": tex.uv("body"), "+y": tex.uv("bezel"),
              "-y": None, "+x": tex.uv("bezel"), "-x": tex.uv("bezel")},
          color=frame)
    # Screen: 5 mm inside the bezel opening on every side and 2 mm proud of the
    # housing, so the recess reads as a dark slot around the glass.
    p.plane((0.0, 0.35, housing_front + 0.002), (0.99, 0.57, 0.0), normal="z",
            uv=tex.uv("screen"), color=_tint(screen, 0.45))
    # Two feet and a cross-bar: a token stand, all of it inside the silhouette.
    for sx in (-1.0, 1.0):
        p.box((sx * 0.42, 0.010, 0.0), (0.06, 0.020, 0.06), uv=tex.uv("body"),
              color=_tint(dark, 0.35), proxy=False)
    p.box((0.0, 0.011, -0.010), (0.78, 0.018, 0.05), uv=tex.uv("body"),
          color=_tint(dark, 0.35), proxy=False)
    p.add_note("flat panel: bezel bars carry the size, screen is one recessed quad")


PROPS = {
    "core:plant": build_plant,
    "core:rug": build_rug,
    "core:lamp": build_lamp,
    "core:tv": build_tv,
}

"""Pool prop parts.

The Pool family: white moulded-resin patio furniture, a chrome pool ladder,
pale privacy-curtain screens and silver guardrails.  Each entry is
``{"core:<id>": build_function}`` exactly like the other part modules; the
catalogue is the authoritative list of ids and sizes.

The set is one family: a clean, relatively new, sterile institutional pool.
Everything is pale -- white resin furniture, chrome ladder, dull-silver
guardrails and pale privacy curtains -- and the wear is deliberately light (a
few faint scuffs, no rust, no mould) because the Pool is empty, not abandoned.
Colours come from :mod:`palette`; the catalogue ``size`` is the authoritative
bounding box and the origin is the floor-contact centre.

Construction conventions
------------------------

* **Modular bays on a 0.6 m grid.**  The straight / end / corner modules of
  both the curtain and the guardrail compose into runs.  Every module puts its
  end posts inboard by the post radius (or by the foot-plate half-width for the
  curtains) so the post surface, and a guardrail's base flange, are flush with
  the module's catalogue edge.  Two modules placed edge to edge in a level
  therefore meet piece to piece with no gap and no overlap, and their rails
  butt into one continuous line.
* **A rail always dies at a post.**  A guardrail or curtain rail spans its
  whole module and terminates inside (or immediately behind) the end post, so a
  run reads as one continuous rail and a lone module still shows a finished
  end.  No rail is left with a raw open end.
* **One guardrail rail.**  The guardrail is a single waist-high (1.05 m) pipe
  rail: three posts to a 2 m bay, one Ø42 rail at ~0.98 m, Ø48 posts with
  turned caps and bolted rectangular base flanges.  It is deliberately *not* a
  two-rail fence.
* **Moulded resin is faceted, metal is turned.**  Table and chair legs are
  four-sided tapered blocks (a 4-segment lathe rotated 45 degrees) rather than
  round tubes; metal work is eight-sided tube or round stock with painted
  lengthwise highlights, so the two material families never read alike.
* **Cloth is gathered.**  Curtain pleats are a double-sided zigzag ribbon that
  is nearly flat where it meets the track and opens to the full 0.22 m pleat
  depth at the hem, and small roller carriers sit over the pleat crests.
* **+Z is each module's front**: the ladder's handrails curve towards +Z (over
  the deck edge), the curtain pleats open towards +Z, and the guardrail and
  curtain faces are symmetric about it.
"""

from __future__ import annotations

import math

import palette
from mesh import FACE_KEYS, PropBuilder

# ---------------------------------------------------------------- budgets
#
# Pack budget: 500 preferred, 800 review, 1500 hard (tools/props/README.md).
# Each value is the target this module designs to, not a soft hint.  The
# guardrails and the ladder spend their triangles on turned caps, flanges and
# treads rather than on extra sides: eight-sided stock throughout.

TARGETS = {
    "core:pool_table": 190,
    "core:pool_chair": 260,
    "core:pool_ladder": 380,
    "core:pool_curtain_straight": 240,
    "core:pool_curtain_end": 150,
    "core:pool_curtain_corner": 240,
    "core:pool_guardrail_straight": 220,
    "core:pool_guardrail_end": 160,
    "core:pool_guardrail_corner": 240,
}

# ------------------------------------------------------------------ palette

RESIN = (234, 231, 223)          # the resin's albedo, a touch above the dingy
RESIN_TINT = palette.hex_to_rgb(palette.PLASTIC_WHITE)  # palette white vertex tint
CLOTH = palette.mix(
    palette.hex_to_rgb(palette.PLASTIC_WHITE),
    palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL),
    0.16,
)
CLOTH_TINT = palette.mix(CLOTH, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.35)
CHROME = palette.hex_to_rgb(palette.CHROME)
CHROME_TINT = palette.mix(CHROME, palette.hex_to_rgb(palette.WALL_CREAM), 0.30)
SILVER = palette.hex_to_rgb(palette.METAL_LIGHT)
SILVER_TINT = palette.mix(SILVER, palette.hex_to_rgb(palette.WALL_CREAM), 0.28)

# ------------------------------------------------------------- guardrail stock
#
# One stock section for all three guardrail modules, so a run cannot drift: the
# 1.05 m post top, the 0.98 m rail height and the flange size are shared.

POST_R = 0.024                   # Ø48 post
POST_TOP = 1.05                  # catalogue height: the post cap tops out here
POST_CAP_H = 0.022               # turned cap on the post
POST_CAP_TAPER = 0.66
RAIL_R = 0.021                   # Ø42 top rail
RAIL_Y = 0.98                    # rail axis; the rail top sits 49 mm under the cap
PLATE_L = 0.048                  # base flange along the rail direction
PLATE_D = 0.08                   # base flange across it (this fills the 8 cm depth)
PLATE_H = 0.014

METAL_SEGMENTS = 8

# --------------------------------------------------------------- curtain stock

CURTAIN_POST_R = 0.024           # Ø48 post
CURTAIN_FOOT = 0.06              # square foot plate; its half-width sets the inset
CURTAIN_FOOT_H = 0.014
CURTAIN_TOP = 2.60               # catalogue height
CURTAIN_CAP_H = 0.025
CURTAIN_CAP_TAPER = 0.62
TRACK_W = 0.035                  # extruded track, across
TRACK_H = 0.02
TRACK_Y = 2.50                   # track centre; the post rises 0.10 m above it
PANEL_TOP = 2.49                 # the cloth hangs from the track's underside
PANEL_BOTTOM = 0.06              # a short, believable gap above the floor
PLEAT_GATHER = 0.014             # pleat depth at the gathered top
CARRIER = (0.026, 0.05, 0.016)   # roller carrier under the track


# ------------------------------------------------------------------ helpers


def _box(p: PropBuilder, center, size, uv, color, hidden=("-y",), colors=None, rotation=None) -> None:
    """A box that skips never-visible faces (each hidden face is 2 triangles)."""
    faces = dict(uv) if isinstance(uv, dict) else {key: uv for key in FACE_KEYS}
    for key in hidden:
        faces[key] = None
    p.box(center, size, uv=faces, color=color, colors=colors, rotation=rotation)


def _turn(p: PropBuilder, base, radius: float, height: float, uv, color, *,
          taper: float = 1.0, bottom: bool = False) -> None:
    """A turned metal section: a cone or tube with its top cap."""
    p.cylinder(
        base, radius, height, segments=METAL_SEGMENTS, taper=taper,
        side_uv=uv, cap_uv=uv, color=color, bottom=bottom,
    )


def _post(p: PropBuilder, x: float, z: float, radius: float, top: float, cap_h: float,
          cap_taper: float, base: float, post_uv, cap_uv, post_color, cap_color) -> None:
    """A post rising from ``base`` with a turned cap at the catalogue top."""
    shaft_top = top - cap_h
    p.cylinder(
        (x, base, z), radius, shaft_top - base, segments=METAL_SEGMENTS,
        side_uv=post_uv, cap_uv=post_uv, color=post_color, bottom=False,
    )
    _turn(p, (x, shaft_top, z), radius, cap_h, cap_uv, cap_color, taper=cap_taper)


def _flange(p: PropBuilder, x: float, z: float, plate_uv, color) -> None:
    """A bolted rectangular base flange; its depth fills the catalogue axis."""
    _box(p, (x, PLATE_H * 0.5, z), (PLATE_L, PLATE_H, PLATE_D), plate_uv, color)


def _rail_x(p: PropBuilder, x0: float, x1: float, z: float, y: float, radius: float,
            rail_uv, color) -> None:
    """A horizontal rail along X, capped at both ends."""
    p.cylinder(
        (min(x0, x1), y, z), radius, abs(x1 - x0), axis="x", segments=METAL_SEGMENTS,
        side_uv=rail_uv, cap_uv=rail_uv, color=color, bottom=True,
    )


def _rake(p: PropBuilder, first_vertex: int, pivot, degrees: float) -> None:
    """Leans every vertex added since ``first_vertex`` about ``pivot`` (X axis).

    The chair's rear legs and its tapered blocks are built upright and then
    leaned, which keeps the tapered-block primitive simple.  Positive degrees
    lean the part's far end towards -Z.
    """
    radians = math.radians(degrees)
    cos, sin = math.cos(radians), math.sin(radians)
    _, py, pz = pivot
    for index in range(first_vertex, len(p.mesh.positions)):
        x, y, z = p.mesh.positions[index]
        dy, dz = y - py, z - pz
        p.mesh.positions[index] = (x, py + dy * cos - dz * sin, pz + dy * sin + dz * cos)


def _taper_block(p: PropBuilder, base, widths, height: float, uv, color, *,
                 rake: float = 0.0, proxy: bool = True) -> int:
    """A tapered four-sided block: the moulded-resin leg / stile primitive.

    ``widths`` is ``(bottom, top)`` measured across the flats; the 4-segment
    lathe is rotated 45 degrees so the flats face the axes and the corners
    carry the silhouette, which is what makes moulded furniture read as moulded
    rather than as extruded tube.  ``rake`` leans the block about its top
    towards -Z.  Returns the index of the block's first vertex.
    """
    bottom, top = widths
    first = len(p.mesh.positions)
    p.lathe(
        base,
        [(0.0, bottom / math.sqrt(2.0)), (height, top / math.sqrt(2.0))],
        segments=4,
        rotation=math.radians(45.0),
        uv=uv,
        cap_uv=uv,
        color=color,
        proxy=False,
    )
    if rake:
        _rake(p, first, (base[0], base[1] + height, base[2]), rake)
    if proxy:
        # The lathe's own proxy cannot express the rake, so emit a tube proxy
        # (the editor supports arbitrary orientation) covering the block.
        dx = math.sin(math.radians(rake)) * height
        p.mesh.parts.append({
            "shape": "tube",
            "start": [round(base[0], 4), round(base[1], 4), round(base[2] + dx, 4)],
            "end": [round(base[0], 4), round(base[1] + height * math.cos(math.radians(rake)), 4),
                    round(base[2], 4)],
            "radius": round((bottom + top) * 0.25, 4),
            "color": _hex(color),
        })
    return first


def _hex(color) -> str:
    return "#%02x%02x%02x" % tuple(max(0, min(255, int(channel))) for channel in color)


# ----------------------------------------------------------------- painting


def _paint_resin(tex, region: str, base, seed: int, wear: float = 0.5) -> None:
    """Clean moulded resin: flat albedo, a faint moulding sheen, light scuffs."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=3, freq=5, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.90), seed=seed + 2, density=0.20, alpha=24)
    tex.grain(region, palette.shade(base, 1.05), seed=seed + 3, density=0.14, alpha=16)
    tex.spots(
        region,
        palette.hex_to_rgb(palette.GRIME),
        count=max(1, int(round(2 * wear))),
        seed=seed + 4,
        radius=2,
        alpha=14,
    )
    tex.border(region, palette.shade(base, 0.88), width=1, alpha=40)


def _paint_tray(tex, region: str, base, seed: int) -> None:
    """The table's tray floor: flat, with a soft shadow where it meets the rim."""
    tex.fill(region, base, jitter=3, seed=seed)
    tex.noise(region, amount=2, freq=6, seed=seed + 1)
    for width, alpha in ((1, 96), (2, 48), (3, 24)):
        tex.border(region, palette.shade(base, 0.90), width=width, alpha=alpha)
    tex.border(region, palette.shade(base, 1.04), width=1, alpha=36)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=1, seed=seed + 2, radius=2, alpha=12)


def _paint_cloth(tex, region: str, base, seed: int) -> None:
    """Pale commercial curtain cloth: a faint weave, a header and a hem.

    The quad mapping puts the region's small-V end at the panel top, so the
    header band (with its grommet dots) is painted at v 0..0.055 and the double
    hem at v 0.88..1.0.
    """
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=4, freq=4, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.90), seed=seed + 2, density=0.22, alpha=22)
    tex.grain(region, palette.shade(base, 1.06), seed=seed + 3, density=0.16, alpha=16)
    tex.band(region, palette.shade(base, 0.87), 0.0, 0.05, alpha=85)
    tex.dots(
        region,
        palette.shade(base, 0.72),
        [(0.06 + index * 0.22, 0.024) for index in range(5)],
        radius=1,
        alpha=230,
    )
    tex.band(region, palette.shade(base, 0.90), 0.885, 0.945, alpha=70)
    tex.band(region, palette.shade(base, 0.78), 0.945, 1.0, alpha=85)
    tex.border(region, palette.shade(base, 0.86), width=1, alpha=30)


def _paint_tube(tex, region: str, base, seed: int, warm: bool = False) -> None:
    """Round metal stock: a lengthwise gradient and a soft specular line.

    A cylinder wraps ``u`` around its circumference and runs ``v`` along the
    axis, so a vertical bar in the region becomes a highlight *line* down the
    part and a horizontal band becomes a ring at that point along it.
    """
    tex.gradient(region, palette.shade(base, 1.04), palette.shade(base, 0.90), jitter=3, seed=seed)
    tex.bar(region, palette.shade(base, 1.10), (0.22, 0.0, 0.44, 1.0), alpha=58)
    tex.bar(region, palette.shade(base, 1.16), (0.30, 0.0, 0.36, 1.0), alpha=52)
    tex.bar(region, palette.shade(base, 0.88), (0.64, 0.0, 0.78, 1.0), alpha=40)
    tex.grain(region, palette.shade(base, 0.84), seed=seed + 1, density=0.26, alpha=26)
    tex.grain(region, palette.shade(base, 1.12), seed=seed + 2, density=0.16, alpha=16)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=2, seed=seed + 3, radius=1, alpha=12)
    if warm:
        tex.spots(region, palette.hex_to_rgb(palette.RUST), count=1, seed=seed + 4, radius=1, alpha=10)
    tex.border(region, palette.shade(base, 0.88), width=1, alpha=26)


def _paint_flange(tex, region: str, base, seed: int, bolts: int = 4) -> None:
    """A cast base flange: a brushed field, a rim highlight and bolt caps."""
    tex.fill(region, palette.shade(base, 0.96), jitter=4, seed=seed)
    tex.gradient(region, palette.shade(base, 1.02), palette.shade(base, 0.90), jitter=2, seed=seed + 1)
    tex.border(region, palette.shade(base, 0.80), width=1, alpha=60)
    tex.border(region, palette.shade(base, 1.06), width=1, alpha=26)
    inset = 0.16
    corners = [
        (inset, inset),
        (1.0 - inset, inset),
        (inset, 1.0 - inset),
        (1.0 - inset, 1.0 - inset),
    ]
    for fx, fy in corners[:bolts]:
        tex.dots(region, palette.shade(base, 1.24), [(fx, fy)], radius=2, alpha=215)
        tex.dots(region, palette.shade(base, 0.66), [(fx + 0.03, fy + 0.03)], radius=1, alpha=190)
    tex.grain(region, palette.shade(base, 0.86), seed=seed + 2, density=0.24, alpha=22)


# ------------------------------------------------------------------ cloth


def _panel(p: PropBuilder, axis: str, span, hang: float, depth: float, folds: int,
           uv, color) -> None:
    """A gathered curtain panel: a double-sided folded ribbon.

    ``axis`` is the direction the panel spans (``x`` or ``z``); ``hang`` is the
    fold line it is gathered against and ``depth`` how far the pleats open away
    from it.  At the track the folds are almost closed (``PLEAT_GATHER``), and
    they open to ``depth`` at the hem, so the panel hangs off the track and fans
    towards the floor the way gathered cloth does.  Adjacent folds bake slightly
    different shade multipliers exactly the way a real pleat catches the light,
    and every quad is emitted twice (reversed winding) so the panel is solid
    from both sides without paying for a closed shell.
    """
    start, end = span
    distance = end - start
    length = abs(distance)
    closed = [start + distance * (index / folds) for index in range(folds + 1)]
    gathered = [PLEAT_GATHER if index % 2 else 0.0 for index in range(folds + 1)]
    opened = [depth if index % 2 else 0.0 for index in range(folds + 1)]
    top = PANEL_TOP
    bottom = PANEL_BOTTOM

    for index in range(folds):
        u0 = uv[0] + (uv[2] - uv[0]) * (index / folds)
        u1 = uv[0] + (uv[2] - uv[0]) * ((index + 1) / folds)
        rect = (u0, uv[1], u1, uv[3])
        mult = 1.0 if index % 2 == 0 else 0.83
        a, b = closed[index], closed[index + 1]
        gathered_a, gathered_b = hang + gathered[index], hang + gathered[index + 1]
        opened_a, opened_b = hang + opened[index], hang + opened[index + 1]
        if axis == "x":
            front = ((a, bottom, opened_a), (b, bottom, opened_b),
                     (b, top, gathered_b), (a, top, gathered_a))
        else:
            front = ((opened_a, bottom, a), (opened_b, bottom, b),
                     (gathered_b, top, b), (gathered_a, top, a))
        back = (front[1], front[0], front[3], front[2])
        for corners in (front, back):
            p.mesh.quad(*corners, uv=rect, color=color, shade_mult=mult, ao=1.0)

    # One coarse proxy for the whole panel: the editor needs the mass, not the
    # pleats.
    middle = start + distance * 0.5
    height = top - bottom
    if axis == "x":
        center = (middle, (top + bottom) * 0.5, hang + depth * 0.5)
        size = (length, height, depth)
    else:
        center = (hang + depth * 0.5, (top + bottom) * 0.5, middle)
        size = (depth, height, length)
    p.mesh.parts.append({
        "shape": "box",
        "center": [round(value, 4) for value in center],
        "size": [round(value, 4) for value in size],
        "rotation": [0.0, 0.0, 0.0],
        "color": _hex(color),
    })


def _carriers(p: PropBuilder, axis: str, span, hang: float, folds: int, uv, color) -> None:
    """Roller carriers over the gathered pleat crests, tucked under the track."""
    start, end = span
    distance = end - start
    for index in range(1, folds + 1, 2):
        crest = start + distance * (index / folds)
        y = TRACK_Y - TRACK_H * 0.5 - CARRIER[1] * 0.5
        if axis == "x":
            center = (crest, y, hang + PLEAT_GATHER * 0.5)
        else:
            center = (hang + PLEAT_GATHER * 0.5, y, crest)
        _box(p, center, CARRIER, uv, color, hidden=("+y",))


# -------------------------------------------------------------------- table


def build_pool_table(p: PropBuilder) -> None:
    """White resin patio table: a lipped tray top on a moulded skirt, four
    tapered legs and a low perimeter stretcher.  Clean and new."""
    size = p.size  # [0.8, 0.74, 0.8]
    tex = p.set_texture(128, seed=211)
    tex.auto("tray", "trim", "leg", "brace")

    _paint_tray(tex, "tray", RESIN, 301)
    _paint_resin(tex, "trim", palette.shade(RESIN, 0.98), 307, wear=0.4)
    _paint_resin(tex, "leg", palette.shade(RESIN, 0.95), 311, wear=0.8)
    _paint_resin(tex, "brace", palette.shade(RESIN, 0.92), 317, wear=0.9)

    tray_uv = tex.uv("tray")
    trim_uv = tex.uv("trim")
    leg_uv = tex.uv("leg")
    brace_uv = tex.uv("brace")

    top_y = size[1]                  # 0.74
    lip_w = 0.05                     # the tray rim: the top's outer 5 cm
    lip_h = 0.04                     # rim height; the tray floor sits 12 mm down
    slab_h = 0.028
    slab_y = top_y - lip_h           # 0.70: the tray floor slab
    slab_span = size[0] - 0.01       # its sides are buried in the rim

    # Tray floor: one slab whose sides hide in the rim, so the rim reads as one
    # moulding and no two faces are coplanar.
    _box(p, (0.0, slab_y + slab_h * 0.5, 0.0), (slab_span, slab_h, slab_span), tray_uv,
         RESIN_TINT, hidden=("-y", "-x", "+x", "-z", "+z"),
         colors={"+y": palette.shade(RESIN_TINT, 1.03)})

    # The rim: two full-width bars, then two returning between them.
    rim_length = size[2] - 2.0 * lip_w
    for sz in (-1.0, 1.0):
        _box(p, (0.0, slab_y + lip_h * 0.5, sz * (size[2] * 0.5 - lip_w * 0.5)),
             (size[0], lip_h, lip_w), trim_uv, palette.shade(RESIN_TINT, 1.02),
             colors={"+y": palette.shade(RESIN_TINT, 1.05)})
    for sx in (-1.0, 1.0):
        _box(p, (sx * (size[0] * 0.5 - lip_w * 0.5), slab_y + lip_h * 0.5, 0.0),
             (lip_w, lip_h, rim_length), trim_uv, palette.shade(RESIN_TINT, 1.02),
             colors={"+y": palette.shade(RESIN_TINT, 1.05)})

    # A moulded apron under the tray: the shadow gap that makes the top read as
    # a casting rather than a sheet.
    apron_span = size[0] - 0.09
    apron_h = 0.05
    _box(p, (0.0, slab_y - apron_h * 0.5, 0.0), (apron_span, apron_h, apron_span), trim_uv,
         palette.shade(RESIN_TINT, 0.97))

    # Four tapered legs, 46 mm to 34 mm, tucked inside the apron line.
    leg_h = slab_y - apron_h + 0.01
    leg_station = size[0] * 0.5 - 0.06
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            _taper_block(p, (sx * leg_station, 0.0, sz * leg_station), (0.046, 0.034), leg_h,
                         leg_uv, palette.shade(RESIN_TINT, 0.96))

    # A low perimeter stretcher ring, mitred into the legs: four rails that
    # stop inside the leg blocks, so nothing crosses in mid-air.
    brace_y = 0.14
    brace = (0.032, 0.022)
    span = 2.0 * leg_station
    for sz in (-1.0, 1.0):
        _box(p, (0.0, brace_y, sz * leg_station), (span, brace[1], brace[0]), brace_uv,
             palette.shade(RESIN_TINT, 0.94))
    for sx in (-1.0, 1.0):
        _box(p, (sx * leg_station, brace_y, 0.0), (brace[0], brace[1], span - 2.0 * brace[0]),
             brace_uv, palette.shade(RESIN_TINT, 0.94))
    p.add_note("lipped resin tray on a moulded skirt; four tapered legs; perimeter stretcher")


# -------------------------------------------------------------------- chair


def build_pool_chair(p: PropBuilder) -> None:
    """White resin patio chair, the table's sibling: a 45 cm seat with a rolled
    front, rear legs raked back 8 degrees and a slatted back raked 13."""
    size = p.size  # [0.52, 0.85, 0.55]
    tex = p.set_texture(128, seed=223)
    tex.auto("seat", "frame", "leg", "slat")

    _paint_resin(tex, "seat", RESIN, 331, wear=0.4)
    _paint_resin(tex, "frame", palette.shade(RESIN, 0.96), 337, wear=0.7)
    _paint_resin(tex, "leg", palette.shade(RESIN, 0.94), 341, wear=0.9)
    _paint_resin(tex, "slat", palette.shade(RESIN, 0.99), 347, wear=0.3)

    seat_uv = tex.uv("seat")
    frame_uv = tex.uv("frame")
    leg_uv = tex.uv("leg")
    slat_uv = tex.uv("slat")

    seat_top = 0.45
    seat_h = 0.038
    seat_w, seat_d = 0.46, 0.45
    seat_z = 0.02
    seat_front = seat_z + seat_d * 0.5

    # Seat: a moulded slab with a rolled front edge, so the side silhouette is
    # not a plain rectangle.
    _box(p, (0.0, seat_top - seat_h * 0.5, seat_z), (seat_w, seat_h, seat_d), seat_uv,
         RESIN_TINT, colors={"+y": palette.shade(RESIN_TINT, 1.04)})
    p.cylinder((-seat_w * 0.5, seat_top - seat_h, seat_front - 0.012), 0.018, seat_w, axis="x",
               segments=6, side_uv=seat_uv, cap_uv=frame_uv,
               color=palette.shade(RESIN_TINT, 1.0))

    # Seat frame: an apron under the seat on all four sides, tied by the legs.
    apron_y = seat_top - seat_h - 0.022
    apron_h = 0.044
    leg_x = 0.185
    front_z, rear_z = 0.20, -0.19
    for sx in (-1.0, 1.0):
        _box(p, (sx * leg_x, apron_y, (front_z + rear_z) * 0.5),
             (0.028, apron_h, abs(front_z - rear_z) - 0.028), frame_uv,
             palette.shade(RESIN_TINT, 0.96))
    for sz in (-1.0, 1.0):
        _box(p, (0.0, apron_y, front_z if sz > 0 else rear_z),
             (2.0 * leg_x, apron_h, 0.028), frame_uv, palette.shade(RESIN_TINT, 0.96))

    # Legs: tapered blocks; the rear pair leans back 8 degrees about its top so
    # the seat overhangs the feet, the way a real stacker chair does.
    leg_h = apron_y - 0.01
    for sx in (-1.0, 1.0):
        _taper_block(p, (sx * leg_x, 0.0, front_z), (0.042, 0.030), leg_h, leg_uv,
                     palette.shade(RESIN_TINT, 0.97))
        _taper_block(p, (sx * leg_x, 0.0, rear_z), (0.042, 0.030), leg_h, leg_uv,
                     palette.shade(RESIN_TINT, 0.97), rake=-8.0)

    # Back: two raked stiles carrying three slats and a top rail, all on the
    # one 13 degree line so the back reads as a single moulding.
    rake = 13.0
    hinge_y = 0.44
    hinge_z = rear_z

    def back_z(y: float) -> float:
        return hinge_z - (y - hinge_y) * math.tan(math.radians(rake))

    top_y = size[1] - 0.03
    for sx in (-1.0, 1.0):
        centre_y = (hinge_y + top_y) * 0.5
        _box(p, (sx * leg_x, centre_y, back_z(centre_y)),
             (0.034, top_y - hinge_y + 0.03, 0.024), frame_uv, palette.shade(RESIN_TINT, 0.98),
             rotation=(-rake, 0.0, 0.0))
    for y in (0.545, 0.645, 0.745):
        _box(p, (0.0, y, back_z(y)), (0.40, 0.048, 0.016), slat_uv,
             palette.shade(RESIN_TINT, 1.01), rotation=(-rake, 0.0, 0.0))
    _box(p, (0.0, top_y, back_z(top_y)), (0.50, 0.06, 0.022), slat_uv,
         palette.shade(RESIN_TINT, 1.02), rotation=(-rake, 0.0, 0.0))
    p.add_note("45 cm seat with a rolled front, rear legs raked 8 degrees, slatted back")
    p.mesh.normalize_origin()


# ------------------------------------------------------------------- ladder


def build_pool_ladder(p: PropBuilder) -> None:
    """Chrome pool ladder: two Ø48 handrails that rise from the basin floor and
    curve out over the deck edge, with four non-skid treads on a 0.305 m pitch.

    It stands on the basin floor and rises to the 2.2 m catalogue top, which
    puts the grab rail 0.7 m above the deck when the level places the prop at
    the bottom of the basin.
    """
    size = p.size  # [0.55, 2.2, 0.45]
    tex = p.set_texture(128, seed=233)
    tex.auto("tube", "tread", "grip", "boot")

    _paint_tube(tex, "tube", CHROME, 401)
    _paint_resin(tex, "tread", palette.shade(CHROME, 1.04), 407, wear=0.4)
    _paint_resin(tex, "grip", palette.shade(CHROME, 0.70), 411, wear=0.9)
    _paint_tube(tex, "boot", palette.shade(CHROME, 0.84), 417)

    tube_uv = tex.uv("tube")
    tread_uv = tex.uv("tread")
    grip_uv = tex.uv("grip")
    boot_uv = tex.uv("boot")

    rail_r = 0.024
    rail_x = size[0] * 0.5 - rail_r          # 0.251: the rails own the 0.55 m width
    rail_z = -0.20                           # the vertical stock, behind the bend
    bend_r = 0.10
    grab_y = size[1] - rail_r                # 2.176: the rail top reaches the catalogue top
    bend_y = grab_y - bend_r                 # the bend's vertical tangent point
    grab_end = rail_z - rail_r + size[2]     # 0.226: the bend sets the 0.45 m depth

    for sx in (-1.0, 1.0):
        x = sx * rail_x
        points = [(x, 0.02, rail_z), (x, bend_y, rail_z)]
        for step in range(1, 5):
            angle = math.radians(90.0 * (step / 4.0))
            points.append((
                x,
                bend_y + bend_r * math.sin(angle),
                rail_z + bend_r * (1.0 - math.cos(angle)),
            ))
        points.append((x, grab_y, grab_end - 0.045))
        points.append((x, grab_y, grab_end))
        radii = [rail_r] * (len(points) - 1) + [rail_r * 0.6]
        p.tube_path(points, radii=radii, segments=METAL_SEGMENTS, uv=tube_uv,
                    color=CHROME_TINT, cap_start=False, cap_end=True)

        # A vinyl foot boot closes the rail where it meets the basin floor.
        _turn(p, (x, 0.0, rail_z), 0.032, 0.05, boot_uv,
              palette.shade(CHROME_TINT, 0.92), taper=0.86)

    # Four non-skid treads: a stainless pan with a dark insert, on the 0.305 m
    # code pitch and stopping short of the deck above.
    tread_w = 2.0 * (rail_x - rail_r)
    for index in range(4):
        y = 0.35 + index * 0.305
        _box(p, (0.0, y, rail_z), (tread_w, 0.02, 0.075), tread_uv, CHROME_TINT,
             colors={"+y": palette.shade(CHROME_TINT, 1.02)})
        _box(p, (0.0, y + 0.012, rail_z), (tread_w - 0.07, 0.004, 0.048), grip_uv,
             palette.shade(CHROME_TINT, 0.78))
    p.add_note("handrails bend on a 0.10 m radius 0.7 m over the deck; four treads at 0.305 m")


# ----------------------------------------------------------------- curtains


def _curtain_sheet(tex) -> tuple:
    """Paint the shared curtain sheet and return its UV regions."""
    _paint_cloth(tex, "cloth", CLOTH, 501)
    _paint_tube(tex, "post", palette.shade(CLOTH, 0.90), 509)
    _paint_flange(tex, "plate", palette.shade(CLOTH, 0.84), 517, bolts=2)
    _paint_tube(tex, "track", palette.shade(CLOTH, 0.96), 521)
    return tex.uv("cloth"), tex.uv("post"), tex.uv("plate"), tex.uv("track")


def _curtain_track(p: PropBuilder, axis: str, start: float, end: float, station: float,
                   track_uv, color) -> None:
    """The extruded top track: a flat bar spanning the module and joining flush."""
    length = abs(end - start)
    middle = (start + end) * 0.5
    if axis == "x":
        _box(p, (middle, TRACK_Y, station), (length, TRACK_H, TRACK_W), track_uv, color)
    else:
        _box(p, (station, TRACK_Y, middle), (TRACK_W, TRACK_H, length), track_uv, color)


def build_pool_curtain_straight(p: PropBuilder) -> None:
    """Freestanding privacy curtain, full module: two Ø48 posts on square foot
    plates, an extruded top track and a ten-pleat gathered panel."""
    size = p.size  # [1.2, 2.6, 0.22]
    tex = p.set_texture(128, seed=241)
    tex.auto("cloth", "post", "plate", "track")
    cloth_uv, post_uv, plate_uv, track_uv = _curtain_sheet(tex)

    post_x = size[0] * 0.5 - CURTAIN_FOOT * 0.5      # 0.57: the plate is flush at the edge
    for sx in (-1.0, 1.0):
        _box(p, (sx * post_x, CURTAIN_FOOT_H * 0.5, 0.0),
             (CURTAIN_FOOT, CURTAIN_FOOT_H, CURTAIN_FOOT), plate_uv,
             palette.shade(CLOTH_TINT, 0.88))
        _post(p, sx * post_x, 0.0, CURTAIN_POST_R, CURTAIN_TOP, CURTAIN_CAP_H,
              CURTAIN_CAP_TAPER, CURTAIN_FOOT_H * 0.5, post_uv, post_uv,
              CLOTH_TINT, palette.shade(CLOTH_TINT, 1.05))

    _curtain_track(p, "x", -size[0] * 0.5, size[0] * 0.5, 0.0, track_uv,
                   palette.shade(CLOTH_TINT, 0.94))
    edge = post_x - CURTAIN_POST_R                   # the cloth meets the post surface
    folds = 10
    _panel(p, "x", (-edge, edge), -size[2] * 0.5, size[2], folds, cloth_uv, CLOTH_TINT)
    _carriers(p, "x", (-edge, edge), -size[2] * 0.5, folds, post_uv,
              palette.shade(CLOTH_TINT, 0.92))
    p.add_note("ten-pleat gathered panel between two footed posts; the track joins flush")


def build_pool_curtain_end(p: PropBuilder) -> None:
    """Half-width end module that closes a curtain run: one post and its foot
    plate at +X, a half track and a five-pleat panel to the open end."""
    size = p.size  # [0.6, 2.6, 0.22]
    tex = p.set_texture(128, seed=251)
    tex.auto("cloth", "post", "plate", "track")
    cloth_uv, post_uv, plate_uv, track_uv = _curtain_sheet(tex)

    post_x = size[0] * 0.5 - CURTAIN_FOOT * 0.5      # 0.27
    _box(p, (post_x, CURTAIN_FOOT_H * 0.5, 0.0), (CURTAIN_FOOT, CURTAIN_FOOT_H, CURTAIN_FOOT),
         plate_uv, palette.shade(CLOTH_TINT, 0.88))
    _post(p, post_x, 0.0, CURTAIN_POST_R, CURTAIN_TOP, CURTAIN_CAP_H, CURTAIN_CAP_TAPER,
          CURTAIN_FOOT_H * 0.5, post_uv, post_uv, CLOTH_TINT, palette.shade(CLOTH_TINT, 1.05))

    _curtain_track(p, "x", -size[0] * 0.5, size[0] * 0.5, 0.0, track_uv,
                   palette.shade(CLOTH_TINT, 0.94))
    panel = (-size[0] * 0.5 + 0.005, post_x - CURTAIN_POST_R)
    folds = 5
    _panel(p, "x", panel, -size[2] * 0.5, size[2], folds, cloth_uv, CLOTH_TINT)
    _carriers(p, "x", panel, -size[2] * 0.5, folds, post_uv, palette.shade(CLOTH_TINT, 0.92))
    p.add_note("five-pleat panel hangs to the open -X end; the post joins the next module flush")


def build_pool_curtain_corner(p: PropBuilder) -> None:
    """L module turning a run 90 degrees: a shared corner post at (-0.27,
    -0.27), a track along each leg and one five-pleat panel per leg, folded
    inwards so the pleats never cross the module's outer faces."""
    size = p.size  # [0.6, 2.6, 0.6]
    tex = p.set_texture(128, seed=257)
    tex.auto("cloth", "post", "plate", "track")
    cloth_uv, post_uv, plate_uv, track_uv = _curtain_sheet(tex)

    limit = size[0] * 0.5                            # 0.3
    corner = -(limit - CURTAIN_FOOT * 0.5)           # -0.27
    _box(p, (corner, CURTAIN_FOOT_H * 0.5, corner),
         (CURTAIN_FOOT, CURTAIN_FOOT_H, CURTAIN_FOOT), plate_uv,
         palette.shade(CLOTH_TINT, 0.88))
    _post(p, corner, corner, CURTAIN_POST_R, CURTAIN_TOP, CURTAIN_CAP_H, CURTAIN_CAP_TAPER,
          CURTAIN_FOOT_H * 0.5, post_uv, post_uv, CLOTH_TINT, palette.shade(CLOTH_TINT, 1.05))

    _curtain_track(p, "x", -limit, limit, corner, track_uv, palette.shade(CLOTH_TINT, 0.94))
    _curtain_track(p, "z", -limit, limit, corner, track_uv, palette.shade(CLOTH_TINT, 0.94))

    # The panel is gathered against the post's surface and opens inwards, so it
    # stays inside the module's 0.6 x 0.6 m box on both legs.
    gather = corner + CURTAIN_POST_R
    panel = (gather, limit - 0.01)
    depth = 0.20
    folds = 5
    _panel(p, "x", panel, gather, depth, folds, cloth_uv, CLOTH_TINT)
    _carriers(p, "x", panel, gather, folds, post_uv, palette.shade(CLOTH_TINT, 0.92))
    _panel(p, "z", panel, gather, depth, folds, cloth_uv, CLOTH_TINT)
    _carriers(p, "z", panel, gather, folds, post_uv, palette.shade(CLOTH_TINT, 0.92))
    p.add_note("shared corner post; one gathered panel per leg, folded inwards")


# --------------------------------------------------------------- guardrails


def _guardrail_sheet(tex) -> tuple:
    _paint_tube(tex, "post", SILVER, 601)
    _paint_tube(tex, "rail", palette.shade(SILVER, 1.02), 607)
    _paint_flange(tex, "plate", palette.shade(SILVER, 0.90), 613, bolts=4)
    return tex.uv("post"), tex.uv("rail"), tex.uv("plate")


def _guardrail_bay(p: PropBuilder, posts, rail_span, post_uv, rail_uv, plate_uv) -> None:
    """The shared bay construction: a flange and post per station, then one rail.

    Every guardrail module is built from this, so the post stock, the rail
    height and the flange size cannot drift between straight, end and corner.
    """
    for x, z in posts:
        _flange(p, x, z, plate_uv, palette.shade(SILVER_TINT, 0.92))
        _post(p, x, z, POST_R, POST_TOP, POST_CAP_H, POST_CAP_TAPER, PLATE_H * 0.5,
              post_uv, post_uv, SILVER_TINT, palette.shade(SILVER_TINT, 1.06))
    (x0, z0), (x1, z1) = rail_span
    if abs(z1 - z0) < 1e-6:
        _rail_x(p, x0, x1, z0, RAIL_Y, RAIL_R, rail_uv, palette.shade(SILVER_TINT, 1.02))
    else:
        p.cylinder((x0, RAIL_Y, min(z0, z1)), RAIL_R, abs(z1 - z0), axis="z",
                   segments=METAL_SEGMENTS, side_uv=rail_uv, cap_uv=rail_uv,
                   color=palette.shade(SILVER_TINT, 1.02), bottom=True)


def build_pool_guardrail_straight(p: PropBuilder) -> None:
    """A 2 m bay of waist-high guard rail: one Ø42 rail at 0.98 m on three Ø48
    posts with bolted flanges.  One rail, not a fence."""
    size = p.size  # [2.0, 1.05, 0.08]
    tex = p.set_texture(128, seed=261)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_sheet(tex)

    limit = size[0] * 0.5                    # 1.0
    inset = POST_R                           # the post surface is flush at the edge
    posts = ((-(limit - inset), 0.0), (0.0, 0.0), (limit - inset, 0.0))
    _guardrail_bay(p, posts, ((-limit, 0.0), (limit, 0.0)), post_uv, rail_uv, plate_uv)
    p.add_note("single waist-high rail on three posts; flanges flush at the module ends")


def build_pool_guardrail_end(p: PropBuilder) -> None:
    """A 0.6 m guardrail return: the same stock, two posts and one rail, so it
    terminates a run without introducing a second rail line."""
    size = p.size  # [0.6, 1.05, 0.08]
    tex = p.set_texture(128, seed=263)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_sheet(tex)

    limit = size[0] * 0.5
    inset = POST_R
    posts = ((-(limit - inset), 0.0), (limit - inset, 0.0))
    _guardrail_bay(p, posts, ((-limit, 0.0), (limit, 0.0)), post_uv, rail_uv, plate_uv)
    p.add_note("short single-rail return: two posts, flanges flush at both ends")


def build_pool_guardrail_corner(p: PropBuilder) -> None:
    """L guardrail module: one rail per leg at the same waist height, turning
    through the shared corner post."""
    size = p.size  # [0.6, 1.05, 0.6]
    tex = p.set_texture(128, seed=267)
    tex.auto("post", "rail", "plate")
    post_uv, rail_uv, plate_uv = _guardrail_sheet(tex)

    limit = size[0] * 0.5
    corner = -(limit - POST_R)
    posts = ((corner, corner), (limit - POST_R, corner), (corner, limit - POST_R))
    _guardrail_bay(p, posts, ((-limit, corner), (limit, corner)), post_uv, rail_uv, plate_uv)
    p.cylinder((corner, RAIL_Y, -limit), RAIL_R, 2.0 * limit, axis="z",
               segments=METAL_SEGMENTS, side_uv=rail_uv, cap_uv=rail_uv,
               color=palette.shade(SILVER_TINT, 1.02), bottom=True)
    p.add_note("shared corner post; one waist-high rail per leg; flanges flush at the ends")


PROPS = {
    "core:pool_table": build_pool_table,
    "core:pool_chair": build_pool_chair,
    "core:pool_ladder": build_pool_ladder,
    "core:pool_curtain_straight": build_pool_curtain_straight,
    "core:pool_curtain_end": build_pool_curtain_end,
    "core:pool_curtain_corner": build_pool_curtain_corner,
    "core:pool_guardrail_straight": build_pool_guardrail_straight,
    "core:pool_guardrail_end": build_pool_guardrail_end,
    "core:pool_guardrail_corner": build_pool_guardrail_corner,
}

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
* **Cloth is gathered.**  A curtain panel is a double-sided folded ribbon built
  in two bands: the top 0.26 m fans out from the track (pleats almost closed
  where they hang) and the body below hangs with vertical fold faces at the
  full depth, which is what real gathered cloth does and what keeps the painted
  header tape and hem square on the model.  Small roller carriers sit over the
  gathered pleat crests.
* **+Z is each module's front**: the ladder's handrails curve towards +Z (over
  the deck edge), the curtain pleats open towards +Z, and the guardrail and
  curtain faces are symmetric about it.
"""

from __future__ import annotations

import math
from pathlib import Path

import palette
from mesh import FACE_KEYS, PropBuilder
from parts.refreshed import load_atlas_from, outward_lathe

# ---------------------------------------------------------------- budgets
#
# Pack budget: 500 preferred, 800 review, 1500 hard (tools/props/README.md).
# Each value is the target this module designs to, not a soft hint.  The
# guardrails and the ladder spend their triangles on turned caps, flanges and
# treads rather than on extra sides: eight-sided stock throughout.

TARGETS = {
    "core:pool_table": 190,
    "core:pool_chair": 260,
    "core:pool_ladder": 440,
    "core:pool_curtain_straight": 280,
    "core:pool_curtain_end": 150,
    "core:pool_curtain_corner": 240,
    "core:pool_guardrail_straight": 220,
    "core:pool_guardrail_end": 160,
    "core:pool_guardrail_corner": 240,
    "core:rubber_duck": 208,
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
PANEL_FAN = 0.26                 # how much of the top is still gathered/still opening
PLEAT_GATHER = 0.014             # pleat depth at the gathered top
CORNER_DEPTH = 0.12              # the tighter pleat a corner panel packs into
CARRIER = (0.026, 0.05, 0.016)   # roller carrier under the track
CORNER_STACK = 0.03              # second corner panel's offset from the post face


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
                 rake: float = 0.0) -> None:
    """A tapered four-sided block: the moulded-resin leg / stile primitive.

    ``widths`` is ``(bottom, top)`` measured across the flats; the 4-segment
    lathe is rotated 45 degrees so the flats face the axes and the corners
    carry the silhouette, which is what makes moulded furniture read as moulded
    rather than as extruded tube.  ``rake`` leans the block about its top
    towards -Z.
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
    )
    if rake:
        # A raked leg is cut square at the floor: without this the tilted foot
        # would dip below y = 0 and the prop's origin shift would lift its
        # upright siblings off the ground.
        foot = [
            index
            for index in range(first, len(p.mesh.positions))
            if abs(p.mesh.positions[index][1] - base[1]) < 1e-6
        ]
        _rake(p, first, (base[0], base[1] + height, base[2]), rake)
        for index in foot:
            x, _, z = p.mesh.positions[index]
            p.mesh.positions[index] = (x, 0.0, z)


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


# ------------------------------------------------------------------ cloth


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

    # The rim: two full-width bars, then two returning between them.  The
    # returns run past the full-width bars' inner faces so no two faces end up
    # coplanar.
    rim_length = size[2] - lip_w
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
    # run through the leg blocks, so every joint is closed.
    brace_y = 0.14
    brace = (0.032, 0.022)
    span = 2.0 * leg_station
    for sz in (-1.0, 1.0):
        _box(p, (0.0, brace_y, sz * leg_station), (span, brace[1], brace[0]), brace_uv,
             palette.shade(RESIN_TINT, 0.94))
    for sx in (-1.0, 1.0):
        _box(p, (sx * leg_station, brace_y, 0.0), (brace[0], brace[1], span), brace_uv,
             palette.shade(RESIN_TINT, 0.94))
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
    seat_w, seat_d = 0.52, 0.45
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
    leg_x = 0.205
    front_z, rear_z = 0.20, -0.19
    for sx in (-1.0, 1.0):
        _box(p, (sx * leg_x, apron_y, (front_z + rear_z) * 0.5),
             (0.028, apron_h, abs(front_z - rear_z) - 0.028), frame_uv,
             palette.shade(RESIN_TINT, 0.96))
    for sz in (-1.0, 1.0):
        _box(p, (0.0, apron_y, front_z if sz > 0 else rear_z),
             (2.0 * leg_x, apron_h, 0.028), frame_uv, palette.shade(RESIN_TINT, 0.96))

    # Legs: tapered blocks; the rear pair leans back 8 degrees about its top so
    # the seat overhangs the feet, the way a real stacker chair does.  The
    # raked pair is grown by the foot's rise so all four feet still touch y = 0.
    leg_h = apron_y - 0.01
    lean = 8.0
    foot_rise = leg_h * (1.0 - math.cos(math.radians(lean)))
    for sx in (-1.0, 1.0):
        _taper_block(p, (sx * leg_x, 0.0, front_z), (0.042, 0.030), leg_h, leg_uv,
                     palette.shade(RESIN_TINT, 0.97))
        _taper_block(p, (sx * leg_x, -foot_rise, rear_z), (0.042, 0.030), leg_h + foot_rise,
                     leg_uv, palette.shade(RESIN_TINT, 0.97), rake=-lean)

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
    for y in (0.52, 0.62, 0.72):
        _box(p, (0.0, y, back_z(y)), (0.40, 0.048, 0.016), slat_uv,
             palette.shade(RESIN_TINT, 1.01), rotation=(-rake, 0.0, 0.0))
    _box(p, (0.0, top_y, back_z(top_y)), (0.46, 0.06, 0.022), slat_uv,
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
        for step in range(1, 7):
            angle = math.radians(90.0 * (step / 6.0))
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
        _turn(p, (x, 0.0, rail_z), 0.028, 0.05, boot_uv,
              palette.shade(CHROME_TINT, 0.92), taper=0.86)

    # Four non-skid treads: a stainless pan with a dark insert, on the 0.305 m
    # code pitch and stopping short of the deck above.  The pan sits on the
    # rails' front face (z + 12 mm) the way a real tread mounts, which also
    # keeps it inside the module's 0.45 m depth.
    tread_w = 2.0 * (rail_x - rail_r)
    for index in range(4):
        y = 0.35 + index * 0.305
        _box(p, (0.0, y, rail_z + 0.012), (tread_w, 0.02, 0.075), tread_uv, CHROME_TINT,
             colors={"+y": palette.shade(CHROME_TINT, 1.02)})
        _box(p, (0.0, y + 0.012, rail_z + 0.012), (tread_w - 0.07, 0.004, 0.048), grip_uv,
             palette.shade(CHROME_TINT, 0.78))
    p.add_note("handrails bend on a 0.10 m radius 0.7 m over the deck; four treads at 0.305 m")


# ------------------------------------------------------------------- hot tub
#
# A genuinely circular hot tub shell: an inner tiled wall from the basin floor
# to the rim, a flat annular rim cap and an outer skirt that reaches below the
# deck line. The level cuts the basin as a *conservative* 32-strip polygon
# inscribed in a 1.25 m circle; its boundary dips to the minimum radius below
# (HOT_TUB_RECESS_MIN_R, computed by hot_tub_recess_min_radius), and the wall
# sits just inside that minimum so every polygon step hides behind it. The
# matching circular water volume uses the full 1.25 m radius, so water
# membership, the collision rim and the drawn disc are the same circle; the
# deck line is 1.50 m above the basin floor and the rim cap covers the rest of
# the polygon out to the 1.30 m outer radius.

HOT_TUB_OUTER_R = 1.30
HOT_TUB_WATER_R = 1.25
HOT_TUB_RECESS_STRIPS = 32
HOT_TUB_WALL_R = 1.1730
HOT_TUB_FLANGE_R = 1.043
HOT_TUB_RIM_TOP = 1.56
HOT_TUB_DECK = 1.50
HOT_TUB_SKIRT_BOTTOM = 1.30
HOT_TUB_SEGMENTS = 48

HOT_TUB_TILE = palette.hex_to_rgb("#8f9fa8")
HOT_TUB_CAP = palette.hex_to_rgb("#4a4e52")


def hot_tub_recess_strips(centre=(0.0, 0.0)) -> list:
    """The level's basin recess: ``floor_regions`` dicts for the map author.

    Each strip is conservative (entirely inside the ``HOT_TUB_WATER_R``
    circle), so the walkable floor never leaves the water circle; the strips'
    steps all stay outside ``HOT_TUB_WALL_R`` and are hidden behind the shell.
    """
    radius = HOT_TUB_WATER_R
    cx, cz = centre
    strips = []
    for index in range(HOT_TUB_RECESS_STRIPS):
        x0 = cx - radius + (2.0 * radius) * index / HOT_TUB_RECESS_STRIPS
        x1 = cx - radius + (2.0 * radius) * (index + 1) / HOT_TUB_RECESS_STRIPS
        edge = max(abs(x0 - cx), abs(x1 - cx))
        half = math.sqrt(max(0.0, radius * radius - edge * edge))
        if half <= 0.01:
            continue
        strips.append(
            {
                "x": round(x0, 4),
                "z": round(cz - half, 4),
                "width": round(x1 - x0, 4),
                "depth": round(2.0 * half, 4),
                "offset_y": -1.5,
                "material": "core:pool_tile_basin_01",
                "edge_material": "core:pool_tile_wall_01",
            }
        )
    return strips


def hot_tub_recess_min_radius() -> float:
    """Smallest radius on the conservative polygon boundary, in metres."""
    radius = HOT_TUB_WATER_R
    best = radius
    for index in range(HOT_TUB_RECESS_STRIPS):
        x0 = -radius + 2.0 * radius * index / HOT_TUB_RECESS_STRIPS
        x1 = -radius + 2.0 * radius * (index + 1) / HOT_TUB_RECESS_STRIPS
        edge = max(abs(x0), abs(x1))
        half = math.sqrt(max(0.0, radius * radius - edge * edge))
        if half <= 0.01:
            continue
        for x in (x0, x1):
            for z in (-half, half):
                best = min(best, math.hypot(x, z))
    return best


def _flip(p: PropBuilder, start: int) -> None:
    """Reverse the winding of every triangle emitted since ``start``."""
    for index in range(start, len(p.mesh.indices), 3):
        p.mesh.indices[index + 1], p.mesh.indices[index + 2] = (
            p.mesh.indices[index + 2],
            p.mesh.indices[index + 1],
        )


def _paint_tub_tile(tex, region: str, base, seed: int) -> None:
    """Small square pool tile: a pale grout grid with gentle wear."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=3, freq=6, seed=seed + 1)
    cols = 8
    for index in range(cols + 1):
        tex.bar(region, palette.shade(base, 0.80), (index / cols, 0.0, index / cols + 0.012, 1.0), alpha=150)
        tex.bar(region, palette.shade(base, 0.80), (0.0, index / cols, 1.0, index / cols + 0.012), alpha=150)
    tex.grain(region, palette.shade(base, 0.90), seed=seed + 2, density=0.18, alpha=22)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 3, radius=2, alpha=18)


def build_hot_tub(p: PropBuilder) -> None:
    """Circular hot tub: a joined tiled basin shell with a dark cap rail.

    Mount: the shell's bottom flange sits on the basin floor at y = 0 (2.6 m
    across; rim top 1.56 m, deck line 1.50 m). Author a 1.50 m floor-region
    recess under it (``hot_tub_recess_strips``) and a circular water volume of
    radius ``HOT_TUB_WATER_R`` with its surface 1.35 m above the basin floor;
    the rim stands 6 cm proud of the deck.
    """
    assert hot_tub_recess_min_radius() > HOT_TUB_WALL_R, "the shell wall must sit inside the recess polygon"
    tex = p.set_texture(128, seed=317)
    tex.auto("tile", "cap")
    _paint_tub_tile(tex, "tile", HOT_TUB_TILE, 401)
    _paint_resin(tex, "cap", HOT_TUB_CAP, 409, wear=0.8)

    tile_uv = tex.uv("tile", inset=1)
    cap_uv = tex.uv("cap", inset=1)
    tile_slot = p.material("tub_tile")
    cap_slot = p.material("tub_cap")

    def ring(along_y: float, r0: float, r1: float, uv, color, *, flip: bool) -> None:
        start = len(p.mesh.indices)
        p.mesh.lathe((0.0, along_y, 0.0), [(0.0, r0), (0.0, r1)], segments=HOT_TUB_SEGMENTS,
                     axis="y", uv=uv, color=color, cap_start=False, cap_end=False)
        if flip:
            _flip(p, start)

    def wall(y0: float, y1: float, radius: float, uv, color, *, flip: bool) -> None:
        start = len(p.mesh.indices)
        p.mesh.lathe((0.0, y0, 0.0), [(0.0, radius), (y1 - y0, radius)],
                     segments=HOT_TUB_SEGMENTS, axis="y", uv=uv, color=color,
                     cap_start=False, cap_end=False)
        if flip:
            _flip(p, start)

    p.begin_material(tile_slot)
    # Inner wall from the basin floor to the rim; the renderer draws both
    # windings, so either flip looks the same from inside and outside.
    wall(0.0, HOT_TUB_RIM_TOP, HOT_TUB_WALL_R, tile_uv, palette.shade(HOT_TUB_TILE, 0.96), flip=False)
    p.begin_material(cap_slot)
    # Flat rim cap: hides the recess polygon steps out to the outer edge.
    ring(HOT_TUB_RIM_TOP, HOT_TUB_WALL_R, HOT_TUB_OUTER_R, cap_uv, palette.shade(HOT_TUB_CAP, 1.06), flip=False)
    p.begin_material(tile_slot)
    # Outer drum from the basin floor to the rim: it encloses the annular void
    # between the wall and the outer radius, so the recess polygon's stepped
    # cut edge is hidden from inside, outside and above.
    wall(0.0, HOT_TUB_RIM_TOP, HOT_TUB_OUTER_R, tile_uv, palette.shade(HOT_TUB_TILE, 0.86), flip=False)
    # A thin base apron 2 mm proud of the deck hides the recess polygon's
    # tessellation fringe at the cut edge and reads as the tub's mounting rim.
    ring(HOT_TUB_DECK + 0.002, HOT_TUB_WATER_R - 0.01, HOT_TUB_OUTER_R + 0.035,
         tile_uv, palette.shade(HOT_TUB_TILE, 0.80), flip=False)

    p.add_note("circular shell: wall radius 1.173 m, rim cap to 1.30 m, rim top 1.56 m, deck line 1.50 m")
    p.add_note("level authors the 1.50 m recess strips and a circular water volume of radius 1.25 m at surface 1.35 m")


# ----------------------------------------------------------------- curtains


from parts import pool_remade, duck_remade

PROPS = {
    "core:pool_table": build_pool_table,
    "core:pool_chair": build_pool_chair,
    "core:pool_ladder": build_pool_ladder,
    "core:hot_tub": build_hot_tub,
    "core:pool_curtain_straight": pool_remade.curtains,
    "core:pool_curtain_end": pool_remade.curtains,
    "core:pool_curtain_corner": pool_remade.curtains,
    "core:pool_guardrail_straight": pool_remade.rails,
    "core:pool_guardrail_end": pool_remade.rails,
    "core:pool_guardrail_corner": pool_remade.rails,
    "core:rubber_duck": duck_remade.build,
}

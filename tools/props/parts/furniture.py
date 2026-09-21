"""The core pack's furniture: couch, armchair, chair, table, desk, bookshelf,
cabinet and bed.

Read ``parts/utility.py`` first: this module follows the same contract (the
catalogue ``size`` is authoritative, the origin is the floor-contact centre,
``+Z`` faces the player, one 64/128 texture per prop, colours from
:mod:`palette`) and the same construction style -- a small number of boxes and
low-segment cylinders whose silhouette carries the read, with the texture
spending its pixels on seams, grain, handles and wear.

The eight props are deliberately one set:

* two upholstered pieces (couch, armchair) sharing a composition and a faded
  brown/tan fabric family, the armchair being the couch compressed into 0.9 m;
* four wooden pieces: chair and table in a warm mid wood, desk in a darker
  veneer, bookshelf in dark worn shelving timber;
* a cabinet with veneered doors and dull metal handles;
* a bed whose frame matches the shelving timber and whose bedding is
  institutional white/grey.

Wear is kept low and even (a few grime spots, dulled edges, no gloss) so no
single prop stands out of the set at 480x272.  Where two pieces meet the
smaller is sunk slightly into the larger so no two faces are coplanar and
nothing can flicker in the depth buffer.
"""

from __future__ import annotations

import palette
from mesh import PropBuilder

# Triangle targets from the pack spec (build.py enforces the 500 preferred /
# 1500 hard budget itself; these are what each prop aims for).
TARGETS = {
    "core:couch": 380,
    "core:armchair": 250,
    "core:chair": 180,
    "core:table": 120,
    "core:desk": 160,
    "core:bookshelf": 200,
    "core:cabinet": 180,
    "core:bed": 220,
}


# --------------------------------------------------------------------- paint


def _tint(color: tuple[int, int, int], lift: float = 0.62) -> tuple[int, int, int]:
    """Vertex colour for a face whose texture is painted in ``color``.

    The shader is ``texture * vertex colour * face shade``; using ``color`` for
    both would multiply it into mud.  Lifting it towards the palette's light
    neutral keeps the texture's fading readable while the baked face shading
    still darkens the sides.
    """
    return palette.mix(color, palette.hex_to_rgb(palette.PLASTIC_WHITE), lift)


def _paint_wood(tex, region: str, base: tuple[int, int, int], seed: int, wear: float = 1.0) -> None:
    """Dull varnished wood: long grain, dusty highlights, a little grime."""
    dark = palette.shade(base, 0.72)
    tex.fill(region, base, jitter=9, seed=seed)
    tex.noise(region, amount=6, freq=3, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.45, alpha=55)
    tex.grain(region, palette.shade(base, 1.22), seed=seed + 3, density=0.28, alpha=30)
    tex.streaks(region, dark, count=3, seed=seed + 4, alpha=20, direction="h")
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=max(2, int(3 * wear)), seed=seed + 5,
              radius=3, alpha=24)
    tex.border(region, dark, width=1, alpha=55)


def _paint_fabric(tex, region: str, base: tuple[int, int, int], seed: int, wear: float = 1.0) -> None:
    """Faded upholstery: flat colour, faint weave, soft worn patches."""
    dark = palette.shade(base, 0.80)
    tex.fill(region, base, jitter=7, seed=seed)
    tex.noise(region, amount=5, freq=4, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.40, alpha=38)
    tex.grain(region, palette.shade(base, 1.14), seed=seed + 3, density=0.30, alpha=26)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=max(2, int(3 * wear)), seed=seed + 4,
              radius=3, alpha=22)
    tex.border(region, dark, width=1, alpha=45)


def _paint_cushion(tex, region: str, base: tuple[int, int, int], seed: int) -> None:
    """Upholstery with a loose cushion's seam read.

    The piping runs along the top edge of every face (the box UVs put the
    region's small-V end at the top of each face), so wide, short faces get a
    clean horizontal seam instead of a squashed picture frame.
    """
    _paint_fabric(tex, region, base, seed)
    tex.band(region, palette.shade(base, 1.16), 0.02, 0.09, alpha=70)
    tex.band(region, palette.shade(base, 0.74), 0.09, 0.17, alpha=55)
    tex.border(region, palette.shade(base, 0.84), width=1, alpha=30)


def _paint_metal(tex, region: str, base: tuple[int, int, int], seed: int) -> None:
    """Dull brushed metal (handles, leg caps): no gloss, a spot of rust."""
    tex.fill(region, base, jitter=8, seed=seed)
    tex.grain(region, palette.shade(base, 0.70), seed=seed + 1, density=0.50, alpha=50)
    tex.grain(region, palette.shade(base, 1.25), seed=seed + 2, density=0.30, alpha=35)
    tex.spots(region, palette.hex_to_rgb(palette.RUST), count=2, seed=seed + 3, radius=2, alpha=26)


def _paint_bedding(tex, region: str, base: tuple[int, int, int], seed: int, stripe: bool = False) -> None:
    """Institutional ticking / sheeting: flat, slightly grubby, one woven band."""
    tex.fill(region, base, jitter=6, seed=seed)
    tex.noise(region, amount=4, freq=5, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.86), seed=seed + 2, density=0.30, alpha=30)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 3, radius=3, alpha=20)
    if stripe:
        tex.band(region, palette.shade(base, 0.72), 0.42, 0.50, alpha=90)
    tex.border(region, palette.shade(base, 0.80), width=1, alpha=55)


# ----------------------------------------------------------------- furniture


def build_couch(p: PropBuilder) -> None:
    """Couch: sprung base on wooden feet, back panel, two padded arms, three
    seat and three back cushions."""
    size = p.size  # [2.0, 0.9, 0.9]
    tex = p.set_texture(128, seed=41)
    tex.auto("body", "seat", "back", "wood")

    body = palette.hex_to_rgb(palette.FABRIC_BROWN)
    seat = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.35)
    back = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.55)
    wood = palette.hex_to_rgb(palette.WOOD_DARK)

    _paint_fabric(tex, "body", body, seed=101)
    _paint_cushion(tex, "seat", seat, seed=107)
    _paint_cushion(tex, "back", back, seed=113)
    _paint_wood(tex, "wood", wood, seed=127, wear=1.4)

    half_w = size[0] * 0.5  # 1.00
    half_d = size[2] * 0.5  # 0.45
    body_uv = tex.uv("body")
    seat_uv = tex.uv("seat")
    back_uv = tex.uv("back")
    wood_uv = tex.uv("wood")

    # Four turned feet carry the base 7 cm off the floor and define y = 0.
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * (half_w - 0.12), 0.0, sz * (half_d - 0.07)), 0.045, 0.075, segments=6,
                       uv=wood_uv, color=_tint(wood, 0.5))
    # Base block: widest and deepest part, so it owns the catalogue footprint.
    p.box((0.0, 0.22, 0.0), (size[0], 0.30, size[2]), uv=body_uv, color=_tint(body))
    # Back panel rises to the full catalogue height; cushions cover its front.
    p.box((0.0, 0.63, -half_d + 0.085), (size[0], 0.54, 0.17), uv=body_uv,
          color=_tint(palette.shade(body, 0.94)))
    # Arms + padded caps: the couch's main silhouette cue.
    for sx in (-1.0, 1.0):
        p.box((sx * (half_w - 0.09), 0.50, 0.05), (0.18, 0.28, 0.70), uv=body_uv, color=_tint(body))
        p.box((sx * (half_w - 0.09), 0.66, 0.05), (0.16, 0.08, 0.62), uv=body_uv,
              color=_tint(palette.shade(body, 1.05)), colors={"+y": _tint(palette.shade(body, 1.10))})
    # Three seat cushions: two clear divisions down the seat.
    for sx in (-1.0, 0.0, 1.0):
        p.box((sx * 0.54, 0.44, 0.12), (0.53, 0.14, 0.64), uv=seat_uv, color=_tint(seat))
        p.box((sx * 0.54, 0.525, 0.12), (0.49, 0.05, 0.60), uv=seat_uv,
              color=_tint(palette.shade(seat, 1.06)))
    # Matching back cushions lean on the back panel.
    for sx in (-1.0, 0.0, 1.0):
        p.box((sx * 0.54, 0.58, -0.23), (0.53, 0.36, 0.12), uv=back_uv, color=_tint(back))
        p.box((sx * 0.54, 0.58, -0.15), (0.47, 0.30, 0.06), uv=back_uv,
              color=_tint(palette.shade(back, 1.08)))
    # Two loose cushions against the arms: the only deliberately soft note.
    for sx in (-1.0, 1.0):
        p.box((sx * 0.56, 0.63, -0.12), (0.32, 0.28, 0.10), uv=body_uv,
              color=_tint(palette.shade(body, 0.90)), rotation=(0.0, 0.0, sx * 12.0))
    p.add_note("three seat/back cushions; turned feet; two arm cushions")


def build_armchair(p: PropBuilder) -> None:
    """Armchair: the couch's vocabulary at 0.9 m, one wide cushion each."""
    size = p.size  # [0.9, 0.9, 0.9]
    tex = p.set_texture(128, seed=53)
    tex.auto("body", "seat", "back", "wood")

    body = palette.hex_to_rgb(palette.FABRIC_BROWN)
    seat = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.35)
    back = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.55)
    wood = palette.hex_to_rgb(palette.WOOD_DARK)

    _paint_fabric(tex, "body", body, seed=201)
    _paint_cushion(tex, "seat", seat, seed=207)
    _paint_cushion(tex, "back", back, seed=213)
    _paint_wood(tex, "wood", wood, seed=227, wear=1.4)

    half_w = size[0] * 0.5  # 0.45
    half_d = size[2] * 0.5  # 0.45
    body_uv = tex.uv("body")
    seat_uv = tex.uv("seat")
    back_uv = tex.uv("back")
    wood_uv = tex.uv("wood")

    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * (half_w - 0.10), 0.0, sz * (half_d - 0.11)), 0.04, 0.075, segments=6,
                       uv=wood_uv, color=_tint(wood, 0.5))
    p.box((0.0, 0.22, 0.0), (size[0], 0.30, size[2]), uv=body_uv, color=_tint(body))
    p.box((0.0, 0.63, -half_d + 0.085), (size[0], 0.54, 0.17), uv=body_uv,
          color=_tint(palette.shade(body, 0.94)))
    for sx in (-1.0, 1.0):
        p.box((sx * (half_w - 0.08), 0.50, 0.05), (0.16, 0.28, 0.70), uv=body_uv, color=_tint(body))
        p.box((sx * (half_w - 0.08), 0.66, 0.05), (0.14, 0.08, 0.62), uv=body_uv,
              color=_tint(palette.shade(body, 1.05)), colors={"+y": _tint(palette.shade(body, 1.10))})
    # One broad seat cushion and one back cushion: the couch's three, merged.
    p.box((0.0, 0.44, 0.12), (0.56, 0.14, 0.64), uv=seat_uv, color=_tint(seat))
    p.box((0.0, 0.525, 0.12), (0.50, 0.07, 0.58), uv=seat_uv, color=_tint(palette.shade(seat, 1.06)))
    p.box((0.0, 0.58, -0.23), (0.56, 0.36, 0.12), uv=back_uv, color=_tint(back))
    p.box((0.0, 0.58, -0.15), (0.50, 0.30, 0.06), uv=back_uv, color=_tint(palette.shade(back, 1.08)))
    # A front apron and one loose cushion, as on the couch.
    p.box((0.0, 0.12, 0.425), (size[0] - 0.04, 0.10, 0.04), uv=body_uv,
          color=_tint(palette.shade(body, 0.90)))
    p.box((0.0, 0.63, -0.12), (0.32, 0.28, 0.10), uv=body_uv,
          color=_tint(palette.shade(body, 0.90)), rotation=(0.0, 0.0, 12.0))
    p.add_note("couch vocabulary at 0.9 m; one seat and one back cushion")


def build_chair(p: PropBuilder) -> None:
    """Chair: wooden seat on four turned legs, two stiles, three back slats."""
    size = p.size  # [0.5, 0.9, 0.5]
    tex = p.set_texture(64, seed=67)
    tex.auto("seat", "wood", "back", "worn")

    wood = palette.hex_to_rgb(palette.WOOD_WARM)
    pale = palette.hex_to_rgb(palette.WOOD_PALE)
    dark = palette.mix(wood, palette.hex_to_rgb(palette.WOOD_DARK), 0.55)

    _paint_wood(tex, "seat", pale, seed=301, wear=1.6)
    _paint_wood(tex, "wood", wood, seed=307)
    _paint_wood(tex, "back", pale, seed=311)
    _paint_wood(tex, "worn", dark, seed=317, wear=1.4)

    seat_uv = tex.uv("seat")
    wood_uv = tex.uv("wood")
    back_uv = tex.uv("back")
    worn_uv = tex.uv("worn")

    # Seat slab: 0.445 m sitting height and the catalogue width exactly.
    p.box((0.0, 0.445, 0.0), (size[0], 0.05, size[2]),
          uv={"+y": seat_uv, "-y": worn_uv, "+x": seat_uv, "-x": seat_uv, "+z": seat_uv, "-z": seat_uv},
          color=_tint(pale), colors={"+y": _tint(palette.shade(pale, 1.08))})
    # Four turned legs; their tops sink into the seat slab.
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * 0.19, 0.0, sz * 0.19), 0.021, 0.44, segments=6, uv=wood_uv, color=_tint(wood))
    # Rear stiles run from below the seat to the full catalogue height.
    for sx in (-1.0, 1.0):
        p.box((sx * 0.185, (0.40 + size[1]) * 0.5, -0.2125), (0.035, size[1] - 0.40, 0.035),
              uv=worn_uv, color=_tint(dark))
    for index in range(3):
        p.box((0.0, 0.56 + index * 0.14, -0.2125), (0.38, 0.07, 0.03), uv=back_uv, color=_tint(pale))
    # Four aprons tie the legs together just under the seat.
    for sz in (-1.0, 1.0):
        p.box((0.0, 0.405, sz * 0.19), (0.38, 0.07, 0.03), uv=worn_uv, color=_tint(dark))
    for sx in (-1.0, 1.0):
        p.box((sx * 0.19, 0.405, 0.0), (0.03, 0.07, 0.38), uv=worn_uv, color=_tint(dark))
    p.add_note("four turned legs, three back slats, four aprons")


def build_table(p: PropBuilder) -> None:
    """Table: one thick top on a four-leg frame with aprons."""
    size = p.size  # [1.4, 0.75, 0.8]
    tex = p.set_texture(64, seed=71)
    tex.auto("top", "wood", "edge", "apron")

    wood = palette.hex_to_rgb(palette.WOOD_MID)
    pale = palette.hex_to_rgb(palette.WOOD_PALE)
    dark = palette.shade(wood, 0.78)

    _paint_wood(tex, "top", pale, seed=401, wear=1.5)
    _paint_wood(tex, "wood", wood, seed=407)
    _paint_wood(tex, "edge", dark, seed=411, wear=1.6)
    _paint_wood(tex, "apron", wood, seed=417)

    top_uv = tex.uv("top")
    wood_uv = tex.uv("wood")
    edge_uv = tex.uv("edge")
    apron_uv = tex.uv("apron")

    # The top slab owns the catalogue width and depth.
    p.box((0.0, 0.725, 0.0), (size[0], 0.05, size[2]),
          uv={"+y": top_uv, "-y": None, "+x": edge_uv, "-x": edge_uv, "+z": edge_uv, "-z": edge_uv},
          color=_tint(pale), colors={"+y": _tint(palette.shade(pale, 1.06))})
    # An inset frame under the slab keeps the top from reading as a floating card.
    p.box((0.0, 0.685, 0.0), (size[0] - 0.10, 0.05, size[2] - 0.10), uv=apron_uv, color=_tint(dark))
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.box((sx * (size[0] * 0.5 - 0.08), 0.36, sz * (size[2] * 0.5 - 0.07)), (0.075, 0.72, 0.075),
                  uv=wood_uv, color=_tint(wood))
    for sz in (-1.0, 1.0):
        p.box((0.0, 0.635, sz * (size[2] * 0.5 - 0.085)), (size[0] - 0.20, 0.07, 0.03), uv=apron_uv,
              color=_tint(wood))
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.095), 0.635, 0.0), (0.03, 0.07, size[2] - 0.18), uv=apron_uv,
              color=_tint(wood))
    p.add_note("slab top, four square legs, four aprons")


def build_desk(p: PropBuilder) -> None:
    """Desk: dark-veneer top on two slab ends, modesty panel, drawer pedestal
    and a shallow knee-hole drawer."""
    size = p.size  # [1.6, 0.75, 0.7]
    tex = p.set_texture(64, seed=79)
    tex.auto("top", "body", "drawer", "metal")

    body = palette.hex_to_rgb(palette.WOOD_DARK)
    top = palette.hex_to_rgb(palette.WOOD_VENEER)
    drawer = palette.shade(body, 1.14)
    metal = palette.hex_to_rgb(palette.METAL_DARK)

    _paint_wood(tex, "top", top, seed=501, wear=1.6)
    _paint_wood(tex, "body", body, seed=507, wear=1.4)
    _paint_wood(tex, "drawer", drawer, seed=511)
    _paint_metal(tex, "metal", metal, seed=517)

    top_uv = tex.uv("top")
    body_uv = tex.uv("body")
    drawer_uv = tex.uv("drawer")
    metal_uv = tex.uv("metal")

    p.box((0.0, 0.7275, 0.0), (size[0], 0.045, size[2]),
          uv={"+y": top_uv, "-y": None, "+x": body_uv, "-x": body_uv, "+z": body_uv, "-z": body_uv},
          color=_tint(top), colors={"+y": _tint(palette.shade(top, 1.05))})
    # Slab ends instead of legs: the desk's institutional read.
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.0225), 0.36, 0.0), (0.045, 0.72, size[2] - 0.04), uv=body_uv,
              color=_tint(body))
    # Modesty panel across the back, clear of the floor.
    p.box((0.0, 0.50, -(size[2] * 0.5 - 0.03)), (size[0] - 0.08, 0.36, 0.03), uv=body_uv,
          color=_tint(palette.shade(body, 0.92)))
    # Pedestal with two drawers and bar handles.
    p.box((-0.52, 0.33, 0.0), (0.46, 0.66, 0.60), uv=body_uv, color=_tint(body))
    for cy in (0.50, 0.27):
        p.box((-0.52, cy, 0.295), (0.40, 0.16, 0.03), uv=drawer_uv, color=_tint(drawer))
        p.box((-0.52, cy, 0.315), (0.12, 0.025, 0.02), uv=metal_uv, color=_tint(metal, 0.45))
    # Knee-hole shelf and the shallow centre drawer under the top.
    p.box((0.30, 0.10, 0.0), (0.60, 0.03, 0.60), uv=body_uv, color=_tint(palette.shade(body, 0.95)))
    p.box((0.25, 0.645, 0.0), (size[0] - 0.60, 0.13, size[2] - 0.12), uv=body_uv, color=_tint(body))
    p.box((0.25, 0.645, size[2] * 0.5 - 0.06), (0.46, 0.10, 0.03), uv=drawer_uv, color=_tint(drawer))
    p.box((0.25, 0.645, size[2] * 0.5 - 0.04), (0.12, 0.025, 0.02), uv=metal_uv,
          color=_tint(metal, 0.45))
    p.add_note("darker veneer than the table; slab ends, modesty panel, pedestal")


def build_bookshelf(p: PropBuilder) -> None:
    """Bookshelf: open shell with four shelves and front lips; no books."""
    size = p.size  # [1.0, 1.8, 0.35]
    tex = p.set_texture(128, seed=83)
    tex.auto("side", "shelf", "back", "front")

    body = palette.hex_to_rgb(palette.WOOD_DARK)
    shelf = palette.mix(body, palette.hex_to_rgb(palette.WOOD_MID), 0.70)
    back = palette.shade(body, 0.70)
    front = palette.shade(body, 1.22)

    _paint_wood(tex, "side", body, seed=601, wear=1.5)
    _paint_wood(tex, "shelf", shelf, seed=607, wear=1.3)
    _paint_wood(tex, "back", back, seed=611, wear=1.8)
    _paint_wood(tex, "front", front, seed=617, wear=1.5)

    side_uv = tex.uv("side")
    shelf_uv = tex.uv("shelf")
    back_uv = tex.uv("back")
    front_uv = tex.uv("front")

    # Two full-height sides define the catalogue height and depth.
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.0175), size[1] * 0.5, 0.0), (0.035, size[1], size[2]),
              uv=side_uv, color=_tint(body))
    p.box((0.0, size[1] - 0.02, 0.0), (size[0] - 0.04, 0.04, size[2]), uv=front_uv, color=_tint(front))
    p.box((0.0, 0.02, 0.0), (size[0] - 0.04, 0.04, size[2]), uv=front_uv, color=_tint(front))
    # Hardboard back: the empty shelves need something dull behind them.
    p.box((0.0, size[1] * 0.5, -(size[2] * 0.5 - 0.01)), (size[0] - 0.04, size[1] - 0.04, 0.015),
          uv=back_uv, color=_tint(back))
    # Four shelves at five equal gaps, each with a front lip.
    gap = (size[1] - 0.22) / 5.0
    for index in range(4):
        cy = 0.04 + gap + 0.0175 + index * (0.035 + gap)
        p.box((0.0, cy, 0.01), (size[0] - 0.04, 0.035, size[2] - 0.05), uv=shelf_uv, color=_tint(shelf))
        p.box((0.0, cy, size[2] * 0.5 - 0.01), (size[0] - 0.04, 0.025, 0.02), uv=front_uv,
              color=_tint(front))
    # A top and a bottom rail stand in for a face frame (one texture seam each).
    p.box((0.0, size[1] - 0.06, size[2] * 0.5 - 0.01), (size[0] - 0.06, 0.04, 0.02), uv=front_uv,
          color=_tint(front))
    p.box((0.0, 0.03, size[2] * 0.5 - 0.01), (size[0] - 0.06, 0.04, 0.02), uv=front_uv,
          color=_tint(front))
    p.add_note("empty institutional shelves: no book geometry")


def build_cabinet(p: PropBuilder) -> None:
    """Cabinet: carcass on short metal feet, two veneered doors, bar handles."""
    size = p.size  # [0.9, 0.85, 0.45]
    tex = p.set_texture(64, seed=89)
    tex.auto("body", "door", "edge", "metal")

    body = palette.hex_to_rgb(palette.WOOD_MID)
    door = palette.hex_to_rgb(palette.WOOD_VENEER)
    edge = palette.shade(door, 0.80)
    metal = palette.hex_to_rgb(palette.METAL_DARK)

    _paint_wood(tex, "body", body, seed=701, wear=1.5)
    # Door leaf, then its recessed panel: seams live on the texture only.
    _paint_wood(tex, "door", door, seed=707, wear=1.5)
    tex.panel("door", edge, rect=(0.12, 0.10, 0.88, 0.90), depth=1, alpha=60)
    _paint_wood(tex, "edge", edge, seed=711, wear=1.4)
    _paint_metal(tex, "metal", metal, seed=717)

    body_uv = tex.uv("body")
    door_uv = tex.uv("door")
    edge_uv = tex.uv("edge")
    metal_uv = tex.uv("metal")

    # Top slab owns the catalogue width and depth; the carcass sits under it.
    p.box((0.0, 0.8275, 0.0), (size[0], 0.045, size[2]), uv=body_uv, color=_tint(body))
    p.box((0.0, 0.7975, -0.01), (size[0] - 0.06, 0.035, size[2] - 0.03), uv=edge_uv, color=_tint(edge))
    p.box((0.0, 0.425, -0.0175), (0.86, 0.73, 0.405), uv=body_uv, color=_tint(body))
    # Kick rail between the front feet, under the doors.
    p.box((0.0, 0.0225, 0.175), (0.72, 0.045, 0.02), uv=edge_uv, color=_tint(edge))
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * 0.36, 0.0, sz * 0.16), 0.022, 0.075, segments=6, uv=metal_uv,
                       color=_tint(metal, 0.5))
    # Two doors, flush with the top slab's front edge, facing +Z.
    door_w = (size[0] - 0.07) * 0.5  # 0.415, with a 1 cm gap between the leaves
    for sx in (-1.0, 1.0):
        p.box((sx * (0.005 + door_w * 0.5), 0.425, size[2] * 0.5 - 0.0325), (door_w, 0.74, 0.025),
              uv=door_uv, color=_tint(door), colors={"+z": _tint(door)})
        p.box((sx * 0.045, 0.425, size[2] * 0.5 - 0.0125), (0.02, 0.14, 0.025), uv=metal_uv,
              color=_tint(metal, 0.45))
    p.add_note("two doors with bar handles; panel seams painted, not modelled")


def build_bed(p: PropBuilder) -> None:
    """Bed: wooden frame on short legs, mattress, folded blanket, one pillow."""
    size = p.size  # [1.4, 0.55, 2.0]
    tex = p.set_texture(128, seed=97)
    tex.auto("frame", "mattress", "linen", "board")

    frame = palette.hex_to_rgb(palette.WOOD_MID)
    board = palette.hex_to_rgb(palette.WOOD_DARK)
    mattress = palette.mix(palette.hex_to_rgb(palette.MATTRESS_WHITE), palette.hex_to_rgb(palette.SHEET_GREY), 0.35)
    linen = palette.hex_to_rgb(palette.FABRIC_GREY)

    _paint_wood(tex, "frame", frame, seed=801, wear=1.4)
    _paint_wood(tex, "board", board, seed=807, wear=1.5)
    _paint_bedding(tex, "mattress", mattress, seed=811, stripe=True)
    _paint_bedding(tex, "linen", linen, seed=817)

    frame_uv = tex.uv("frame")
    board_uv = tex.uv("board")
    mattress_uv = tex.uv("mattress")
    linen_uv = tex.uv("linen")

    # Head at -Z (the prop's back), foot at +Z.  Side rails own the footprint.
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.03), 0.17, 0.0), (0.06, 0.22, size[2]), uv=frame_uv,
              color=_tint(frame))
    for sz in (-1.0, 1.0):
        p.box((0.0, 0.17, sz * (size[2] * 0.5 - 0.04)), (size[0] - 0.16, 0.22, 0.06), uv=frame_uv,
              color=_tint(frame))
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * (size[0] * 0.5 - 0.06), 0.0, sz * (size[2] * 0.5 - 0.08)), 0.03, 0.075,
                       segments=6, uv=board_uv, color=_tint(board, 0.5))
    # Head- and footboard span the catalogue width; the posts hold the height.
    p.box((0.0, 0.33, -(size[2] * 0.5 - 0.025)), (size[0] - 0.04, 0.36, 0.05), uv=board_uv,
          color=_tint(board))
    p.box((0.0, 0.21, size[2] * 0.5 - 0.025), (size[0] - 0.04, 0.30, 0.05), uv=board_uv,
          color=_tint(palette.shade(board, 1.08)))
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.04), (0.10 + size[1]) * 0.5, -(size[2] * 0.5 - 0.04)),
              (0.06, size[1] - 0.10, 0.06), uv=board_uv, color=_tint(board))
    # Mattress slab plus a thin top layer: the piping read, no cloth sim.
    p.box((0.0, 0.33, 0.0), (size[0] - 0.10, 0.14, size[2] - 0.18), uv=mattress_uv,
          color=_tint(mattress))
    p.box((0.0, 0.405, 0.0), (size[0] - 0.16, 0.03, size[2] - 0.24), uv=mattress_uv,
          color=_tint(palette.shade(mattress, 1.05)),
          colors={"+y": _tint(palette.shade(mattress, 1.08))})
    # Folded grey blanket over the foot half, with a heavier fold at its edge.
    p.box((0.0, 0.445, 0.34), (size[0] - 0.06, 0.07, 1.18), uv=linen_uv, color=_tint(linen))
    p.box((0.0, 0.49, -0.21), (size[0] - 0.06, 0.08, 0.10), uv=linen_uv,
          color=_tint(palette.shade(linen, 1.1)))
    # One pillow against the headboard, with a shallow puff on top.
    p.box((0.0, 0.45, -0.76), (0.62, 0.10, 0.34), uv=linen_uv, color=_tint(palette.shade(linen, 1.18)))
    p.box((0.0, 0.51, -0.76), (0.54, 0.04, 0.28), uv=linen_uv, color=_tint(palette.shade(linen, 1.22)))
    p.add_note("headboard posts; simple mattress/blanket/pillow blocks")


PROPS = {
    "core:couch": build_couch,
    "core:armchair": build_armchair,
    "core:chair": build_chair,
    "core:table": build_table,
    "core:desk": build_desk,
    "core:bookshelf": build_bookshelf,
    "core:cabinet": build_cabinet,
    "core:bed": build_bed,
}

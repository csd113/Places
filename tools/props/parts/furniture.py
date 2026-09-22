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

import math

import palette
from mesh import FACE_KEYS, PropBuilder

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


# ------------------------------------------------------------------- helpers


def _solid(p: PropBuilder, center, size, uv, color, hidden=("-y",), colors=None, rotation=None) -> None:
    """A box that skips the faces a prop can never show.

    Every hidden face is two triangles and two vertices of the PocketCHIP
    budget, and internal faces (a cushion's underside, a panel face buried in
    another panel) are never visible from any legal camera angle.
    """
    face_uv = dict(uv) if isinstance(uv, dict) else {key: uv for key in FACE_KEYS}
    for key in hidden:
        face_uv[key] = None
    p.box(center, size, uv=face_uv, color=color, colors=colors, rotation=rotation)


def _split_cell(tex, name: str, extra: str, side: str = "v") -> None:
    """Divides a texture region in two and registers the second half.

    The pack's 64/128 canvases are split by ``tex.auto`` into equal cells; a
    prop that needs one more small swatch (a cushion's contrasting fabric, a
    handle's metal) can take half of an existing cell instead of growing the
    canvas or adding a whole cell.
    """
    x, y, w, h = tex.cell(name)
    if side == "v":
        half = max(8, w // 2)
        tex.region(name, (x, y, half, h))
        tex.region(extra, (x + half, y, w - half, h))
    else:
        half = max(8, h // 2)
        tex.region(name, (x, y, w, half))
        tex.region(extra, (x, y + half, w, h - half))


def _cushion(p: PropBuilder, center, size, uv, color, top: float = 0.03, inset: float = 0.024) -> None:
    """A cushion: the padded block plus its inset top puff.

    The second, smaller box is what stops a sofa seat from reading as a stack
    of crates -- it gives the cushion a soft, slightly domed top edge for two
    extra triangles and no texture work.
    """
    cx, cy, cz = center
    sx, sy, sz = size
    _solid(p, (cx, cy, cz), (sx, sy, sz), uv, color)
    _solid(p, (cx, cy + sy * 0.5 + top * 0.5, cz), (sx - inset * 2, top, sz - inset * 2), uv,
           palette.shade(color, 1.05), colors={"+y": palette.shade(color, 1.09)})


# ----------------------------------------------------------------- furniture


def build_couch(p: PropBuilder) -> None:
    """Couch: wooden feet, a seat frame, panel arms with padded caps, a full
    width back under a crest rail, three seat and three back cushions."""
    size = p.size  # [2.0, 0.9, 0.9]
    tex = p.set_texture(128, seed=41)
    tex.auto("body", "seat", "back", "wood")

    body = palette.shade(palette.hex_to_rgb(palette.FABRIC_BROWN), 0.92)
    seat = palette.hex_to_rgb(palette.FABRIC_TAN)
    back = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.45)
    pillow = palette.mix(palette.hex_to_rgb(palette.FABRIC_OLIVE), body, 0.52)
    wood = palette.hex_to_rgb(palette.WOOD_DARK)

    # The wood cell is split so the throw cushions get their own woven swatch
    # (a shared region would just be the frame fabric multiplied into mud).
    _split_cell(tex, "wood", "pillow")

    _paint_fabric(tex, "body", body, seed=101)
    _paint_cushion(tex, "seat", seat, seed=107)
    _paint_cushion(tex, "back", back, seed=113)
    _paint_wood(tex, "wood", wood, seed=127, wear=1.4)
    _paint_fabric(tex, "pillow", pillow, seed=131)

    half_w = size[0] * 0.5  # 1.00
    half_d = size[2] * 0.5  # 0.45
    body_uv = tex.uv("body")
    seat_uv = tex.uv("seat")
    back_uv = tex.uv("back")
    wood_uv = tex.uv("wood")
    pillow_uv = tex.uv("pillow")

    frame_top = 0.32          # seat platform
    arm_x = half_w - 0.085    # arm centre: outer face owns the footprint
    arm_top = 0.60            # where the padded roll takes over
    arm_front = 0.45          # the arms run to the front edge of the footprint
    arm_back = -0.32          # sunk 3 cm into the back panel
    back_front = -0.29        # the back panel's front face

    # Four block feet, then the seat frame: the frame's front face is the apron
    # that carries the couch's silhouette at floor level.
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            _solid(p, (sx * 0.88, 0.06, sz * 0.36), (0.06, 0.12, 0.06), wood_uv, _tint(wood, 0.5))
    _solid(p, (0.0, 0.22, 0.0), (size[0], frame_top - 0.12, size[2]), body_uv, _tint(body),
           hidden=("-y",))
    # Back panel spans the full width *behind* the arms and rises to the
    # catalogue height; the crest rail caps it and breaks the flat top edge.
    _solid(p, (0.0, 0.60, -half_d + 0.08), (size[0], 0.56, 0.16), body_uv,
           _tint(palette.shade(body, 0.94)), hidden=("-y", "+y"))
    _solid(p, (0.0, 0.87, -half_d + 0.095), (size[0], 0.06, 0.19), body_uv,
           _tint(palette.shade(body, 1.02)), hidden=("-y",),
           colors={"+y": _tint(palette.shade(body, 1.08))})
    # Arms: a panel from the seat platform to 0.60, capped by a 6-segment roll
    # that is the one soft silhouette cue on an otherwise angular frame.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * arm_x, (frame_top + arm_top) * 0.5, (arm_back + arm_front) * 0.5),
               (0.17, arm_top - frame_top, arm_front - arm_back), body_uv, _tint(body),
               hidden=("-y", "+y"))
        p.cylinder((sx * arm_x, arm_top, arm_back - 0.02), 0.085, arm_front - arm_back - 0.01,
                   segments=6, axis="z", side_uv=body_uv, cap_uv=body_uv,
                   color=_tint(palette.shade(body, 1.06)), shades=False, proxy=False)
    # Three seat cushions between the arms, each with a soft top puff.
    for index in range(3):
        cx = (index - 1) * 0.56
        _cushion(p, (cx, frame_top + 0.07, 0.07), (0.535, 0.14, 0.70), seat_uv, _tint(seat))
    # Matching back cushions, leaning on the back panel at slightly different
    # angles, each with a shallow front pad so the row reads as loose cushions
    # instead of one extruded block.
    for index, lean in enumerate((-6.0, -8.0, -5.0)):
        cx = (index - 1) * 0.56
        _solid(p, (cx, 0.64, -0.18), (0.535, 0.36, 0.20), back_uv, _tint(back),
               hidden=("-y",), rotation=(lean, 0.0, 0.0))
        _solid(p, (cx, 0.64, -0.115), (0.47, 0.30, 0.05), back_uv,
               _tint(palette.shade(back, 1.07)), hidden=("-y",), rotation=(lean, 0.0, 0.0))
    # Two loose cushions tucked into the corners: the only deliberately soft
    # note, in the pack's muted olive so they read against the brown frame.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * 0.575, 0.62, -0.03), (0.32, 0.30, 0.13), pillow_uv,
               _tint(pillow), hidden=(), rotation=(0.0, sx * 18.0, sx * -7.0))
    p.add_note("frame, rolled arm panels, crest rail; three seat/back cushions; block feet")


def build_armchair(p: PropBuilder) -> None:
    """Armchair: the couch's vocabulary at 0.9 m, one wide cushion each."""
    size = p.size  # [0.9, 0.9, 0.9]
    tex = p.set_texture(128, seed=53)
    tex.auto("body", "seat", "back", "wood")

    body = palette.shade(palette.hex_to_rgb(palette.FABRIC_BROWN), 0.92)
    seat = palette.hex_to_rgb(palette.FABRIC_TAN)
    back = palette.mix(palette.hex_to_rgb(palette.FABRIC_TAN), body, 0.45)
    pillow = palette.mix(palette.hex_to_rgb(palette.FABRIC_OLIVE), body, 0.52)
    wood = palette.hex_to_rgb(palette.WOOD_DARK)

    _split_cell(tex, "wood", "pillow")

    _paint_fabric(tex, "body", body, seed=201)
    _paint_cushion(tex, "seat", seat, seed=207)
    _paint_cushion(tex, "back", back, seed=213)
    _paint_wood(tex, "wood", wood, seed=227, wear=1.4)
    _paint_fabric(tex, "pillow", pillow, seed=231)

    half_w = size[0] * 0.5  # 0.45
    half_d = size[2] * 0.5  # 0.45
    body_uv = tex.uv("body")
    seat_uv = tex.uv("seat")
    back_uv = tex.uv("back")
    wood_uv = tex.uv("wood")
    pillow_uv = tex.uv("pillow")

    frame_top = 0.32
    arm_top = 0.56
    arm_x = half_w - 0.075
    arm_front = 0.45
    arm_back = -0.32
    back_front = -0.29

    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            _solid(p, (sx * 0.36, 0.06, sz * 0.36), (0.06, 0.12, 0.06), wood_uv, _tint(wood, 0.5))
    _solid(p, (0.0, 0.22, 0.0), (size[0], frame_top - 0.12, size[2]), body_uv, _tint(body),
           hidden=("-y",))
    _solid(p, (0.0, 0.60, -half_d + 0.08), (size[0], 0.56, 0.16), body_uv,
           _tint(palette.shade(body, 0.94)), hidden=("-y", "+y"))
    _solid(p, (0.0, 0.87, -half_d + 0.095), (size[0], 0.06, 0.19), body_uv,
           _tint(palette.shade(body, 1.02)), hidden=("-y",),
           colors={"+y": _tint(palette.shade(body, 1.08))})
    for sx in (-1.0, 1.0):
        _solid(p, (sx * arm_x, (frame_top + arm_top) * 0.5, (arm_back + arm_front) * 0.5),
               (0.15, arm_top - frame_top, arm_front - arm_back), body_uv, _tint(body),
               hidden=("-y", "+y"))
        p.cylinder((sx * arm_x, arm_top, arm_back - 0.02), 0.07, arm_front - arm_back - 0.01,
                   segments=6, axis="z", side_uv=body_uv, cap_uv=body_uv,
                   color=_tint(palette.shade(body, 1.06)), shades=False, proxy=False)
    # One broad seat cushion and one back cushion: the couch's three, merged.
    _cushion(p, (0.0, frame_top + 0.07, 0.07), (0.58, 0.14, 0.70), seat_uv, _tint(seat),
             top=0.035, inset=0.03)
    _solid(p, (0.0, 0.62, -0.18), (0.58, 0.32, 0.18), back_uv, _tint(back), hidden=("-y",),
           rotation=(-7.0, 0.0, 0.0))
    _solid(p, (0.0, 0.62, -0.13), (0.51, 0.26, 0.05), back_uv, _tint(palette.shade(back, 1.07)),
           hidden=("-y",), rotation=(-7.0, 0.0, 0.0))
    # A single loose cushion tucked against one arm.
    _solid(p, (0.15, 0.60, -0.04), (0.30, 0.28, 0.12), pillow_uv,
           _tint(pillow), hidden=(), rotation=(0.0, 24.0, -6.0))
    p.add_note("couch vocabulary at 0.9 m; one seat and one back cushion")


def build_chair(p: PropBuilder) -> None:
    """Chair: an inexpensive five-star task chair.

    A moulded plastic seat and backrest on one centre column over a five-spoke
    base.  The proportions are the graded part: a 0.45 m seat at roughly
    0.45 m sitting height, a 0.43 m backrest rising to the 0.90 m catalogue
    height with a slight recline, and castors that carry the 0.5 m footprint.
    """
    size = p.size  # [0.5, 0.9, 0.5]
    tex = p.set_texture(128, seed=67)
    tex.auto("shell", "pad", "metal", "dark")

    shell = palette.mix(
        palette.hex_to_rgb(palette.PLASTIC_DARK),
        palette.hex_to_rgb(palette.METAL_SHADOW),
        0.45,
    )
    pad = palette.mix(
        palette.hex_to_rgb(palette.FABRIC_GREY),
        palette.hex_to_rgb(palette.PLASTIC_DARK),
        0.35,
    )
    metal = palette.hex_to_rgb(palette.METAL_DARK)
    dark = palette.hex_to_rgb(palette.ELECTRONICS_BLACK)

    def paint_shell(region: str, base, seed: int) -> None:
        """Dull moulded plastic: flat, faint mould texture, a scuff or two."""
        tex.fill(region, base, jitter=6, seed=seed)
        tex.noise(region, amount=3, freq=7, seed=seed + 1)
        tex.grain(region, palette.shade(base, 0.82), density=0.22, alpha=18, seed=seed + 2)
        tex.grain(region, palette.shade(base, 1.14), density=0.18, alpha=14, seed=seed + 3)
        tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=4, radius=3, alpha=13,
                  seed=seed + 4)
        tex.border(region, palette.shade(base, 1.10), width=1, alpha=32)

    paint_shell("shell", shell, seed=601)
    _paint_fabric(tex, "pad", pad, seed=607)
    _paint_metal(tex, "metal", metal, seed=613)
    paint_shell("dark", dark, seed=619)

    shell_uv = tex.uv("shell")
    pad_uv = tex.uv("pad")
    metal_uv = tex.uv("metal")
    dark_uv = tex.uv("dark")

    # Base: hub, five radial spokes and five hard-plastic castor pucks.
    hub_r, hub_h = 0.045, 0.05
    spoke_y, spoke_h, spoke_w = 0.040, 0.026, 0.036
    spoke_inner, castor_r0 = 0.028, 0.216
    castor_r, castor_h = 0.040, 0.038
    spoke_length = castor_r0 - spoke_inner
    spoke_mid = (spoke_inner + castor_r0) * 0.5
    for index in range(5):
        angle = math.radians(90.0 + 72.0 * index)
        cx, cz = math.cos(angle), math.sin(angle)
        p.box((cx * spoke_mid, spoke_y, cz * spoke_mid), (spoke_length, spoke_h, spoke_w),
              uv=metal_uv, color=_tint(metal, 0.50),
              rotation=(0.0, -math.degrees(angle), 0.0))
        p.cylinder((cx * castor_r0, 0.0, cz * castor_r0), castor_r, castor_h, segments=8,
                   uv=dark_uv, color=_tint(dark, 0.45))
    p.cylinder((0.0, 0.0, 0.0), hub_r, hub_h, segments=8, uv=metal_uv, color=_tint(metal, 0.55))
    # One centre column: a wide hub shoulder, a slimmer gas lift, a seat plate.
    p.cylinder((0.0, 0.04, 0.0), 0.030, 0.24, segments=8, uv=metal_uv, color=_tint(metal, 0.55))
    p.cylinder((0.0, 0.28, 0.0), 0.020, 0.12, segments=6, uv=metal_uv, color=_tint(metal, 0.60))
    _solid(p, (0.0, 0.4075, 0.01), (0.24, 0.025, 0.24), metal_uv, _tint(metal, 0.48))

    # Seat: the moulded pan carries the catalogue width, the pad sits on it.
    seat_w, seat_d = 0.45, 0.45
    seat_z = 0.01
    _solid(p, (0.0, 0.4175, seat_z), (seat_w, 0.035, seat_d), shell_uv, _tint(shell, 0.70))
    _solid(p, (0.0, 0.45, seat_z), (0.42, 0.03, 0.42), pad_uv, _tint(pad, 0.62),
           hidden=("-y",), colors={"+y": _tint(palette.shade(pad, 1.06), 0.62)})

    # Back: the whole rake line is one angle, so the stalk, the panel and its
    # front pad lean together instead of reading as a ladder bolted on.
    rake = 7.0
    back_y = 0.75          # panel centre
    back_z = -0.205        # panel centre at that height

    def back_z_at(y: float) -> float:
        return back_z + (back_y - y) * math.tan(math.radians(rake))

    _solid(p, (0.0, 0.545, back_z_at(0.545)), (0.07, 0.23, 0.045), shell_uv, _tint(shell, 0.68),
           hidden=("-y",), rotation=(-rake, 0.0, 0.0))
    _solid(p, (0.0, back_y, back_z), (0.43, 0.30, 0.055), shell_uv, _tint(shell, 0.70),
           hidden=("-y",), rotation=(-rake, 0.0, 0.0))
    _solid(p, (0.0, back_y, back_z + 0.037), (0.35, 0.20, 0.022), pad_uv, _tint(pad, 0.62),
           hidden=("-y",), rotation=(-rake, 0.0, 0.0))
    p.add_note("task chair: 0.43 m raked back to 0.90 m, 0.45 m seat, five-star base with castors")


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
    _solid(p, (0.0, 0.728, 0.0), (size[0], 0.044, size[2]),
           {"+y": top_uv, "-y": None, "+x": edge_uv, "-x": edge_uv, "+z": edge_uv, "-z": edge_uv},
           _tint(pale), hidden=("-y",), colors={"+y": _tint(palette.shade(pale, 1.06))})
    # An inset frame under the slab keeps the top from reading as a floating card.
    _solid(p, (0.0, 0.688, 0.0), (size[0] - 0.10, 0.048, size[2] - 0.10), apron_uv, _tint(dark),
           hidden=("-y", "+y"))
    # Square legs, tapered to a slimmer foot the way a plain timber leg is.
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.cylinder((sx * (size[0] * 0.5 - 0.08), 0.0, sz * (size[2] * 0.5 - 0.07)), 0.045,
                       0.706, segments=4, taper=0.86, rotation=0.7854,
                       side_uv=wood_uv, cap_uv=wood_uv, color=_tint(wood))
    for sz in (-1.0, 1.0):
        p.box((0.0, 0.638, sz * (size[2] * 0.5 - 0.085)), (size[0] - 0.20, 0.07, 0.03), uv=apron_uv,
              color=_tint(wood))
    for sx in (-1.0, 1.0):
        p.box((sx * (size[0] * 0.5 - 0.095), 0.638, 0.0), (0.03, 0.07, size[2] - 0.18), uv=apron_uv,
              color=_tint(wood))
    p.add_note("slab top, four tapered square legs, four aprons")


def build_desk(p: PropBuilder) -> None:
    """Desk: a near-black laminate office desk.

    Construction is deliberately plain panel goods: a 4 cm top over two slab
    ends, a modesty panel across the back, and a shallow full-width drawer
    band under the top with a shadow gap and a finger pull.  Every slab meets
    its neighbour with a real overlap, so no box floats unconnected.

    The texture spends its pixels on the laminate grain, the drawer line and
    the lock plate, never on tiny detail that vanishes at 480x272.
    """
    size = p.size  # [1.6, 0.75, 0.7]
    tex = p.set_texture(128, seed=79)
    tex.auto("top", "body", "drawer", "metal")

    # Dark charcoal laminate: near-black, sheen-free.  The exposed panel edges
    # sit a shade lighter, the way a laminate edge band does.
    charcoal = palette.mix(
        palette.hex_to_rgb(palette.ELECTRONICS_DARK),
        palette.hex_to_rgb(palette.METAL_SHADOW),
        0.35,
    )
    top = palette.mix(charcoal, palette.hex_to_rgb(palette.METAL_DARK), 0.30)
    drawer = palette.shade(charcoal, 1.10)
    metal = palette.hex_to_rgb(palette.METAL_DARK)

    def paint_panel(region: str, base, seed: int, grain: float = 0.22, wear: float = 1.0) -> None:
        """Dull laminate: flat base, faint grain, a little hand wear."""
        tex.fill(region, base, jitter=5, seed=seed)
        tex.noise(region, amount=3, freq=6, seed=seed + 1)
        tex.grain(region, palette.shade(base, 0.74), density=grain, alpha=22, seed=seed + 2)
        tex.grain(region, palette.shade(base, 1.18), density=0.16, alpha=16, seed=seed + 3)
        tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=max(2, int(3 * wear)),
                  radius=3, alpha=13, seed=seed + 4)
        tex.border(region, palette.shade(base, 1.12), width=1, alpha=38)

    paint_panel("top", top, seed=501, grain=0.18, wear=1.4)
    paint_panel("body", charcoal, seed=511, wear=1.2)
    paint_panel("drawer", drawer, seed=521, wear=1.6)
    _paint_metal(tex, "metal", metal, seed=531)
    # The routed finger pull lives at the top of the drawer face (the box UVs
    # put the region's small-V end at the top of each face).
    tex.band("drawer", palette.shade(drawer, 0.42), 0.05, 0.15, alpha=120)
    tex.band("drawer", palette.shade(drawer, 0.72), 0.88, 0.96, alpha=70)
    tex.dots("metal", palette.shade(metal, 0.35), [(0.5, 0.55)], radius=1)

    top_uv = tex.uv("top")
    body_uv = tex.uv("body")
    drawer_uv = tex.uv("drawer")
    metal_uv = tex.uv("metal")

    top_t = 0.04
    body_top = size[1] - top_t           # 0.71
    side_t = 0.045
    side_cx = size[0] * 0.5 - side_t * 0.5
    side_depth = size[2] - 0.06          # 0.64, inset under the top's overhang

    # The top owns the full catalogue footprint; its edges are the only place
    # the 4 cm slab is seen, so they share the top's laminate.
    _solid(p, (0.0, size[1] - top_t * 0.5, 0.0), (size[0], top_t, size[2]), top_uv,
           _tint(top, 0.78), hidden=("-y",),
           colors={"+y": _tint(top, 0.82)})
    # Two slab ends, floor to the underside of the top.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * side_cx, body_top * 0.5, 0.0), (side_t, body_top, side_depth), body_uv,
               _tint(charcoal, 0.78), hidden=("-y",))
    # Modesty panel across the back, clear of the floor, sunk into both ends.
    _solid(p, (0.0, 0.42, -(side_depth * 0.5 - 0.02)), (1.53, 0.44, 0.02), body_uv,
           _tint(palette.shade(charcoal, 0.94), 0.78), hidden=("-y",))
    # Drawer housing: a rail between the ends, recessed behind their front
    # edge, that the drawer face closes off.
    _solid(p, (0.0, 0.635, 0.27), (1.53, 0.15, 0.06), body_uv,
           _tint(palette.shade(charcoal, 0.90), 0.78), hidden=("-y",))
    # The drawer face: near-flush with the slab ends, with a 1.8 cm shadow gap
    # left above and below it.
    _solid(p, (0.0, 0.635, 0.313), (1.42, 0.115, 0.02), drawer_uv,
           _tint(drawer, 0.78), hidden=("-y",))
    # A small metal lock plate, sunk into the drawer face.
    p.box((0.60, 0.662, 0.324), (0.05, 0.045, 0.008), uv=metal_uv, color=_tint(metal, 0.5))
    p.add_note("charcoal laminate: 4 cm top, slab ends, modesty panel, shallow drawer band")


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
    """Bed: wooden frame on stub legs, mattress, a blanket draped over the
    foot half and two pillows against a raised headboard."""
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

    half_w = size[0] * 0.5   # 0.70
    half_d = size[2] * 0.5   # 1.00
    # Vertical plan of the bed, from the frame up to the 0.55 m catalogue top.
    rail_bottom, rail_top = 0.08, 0.28
    mattress_top = 0.42
    sheet_top = 0.45
    pillow_top = 0.53

    # Head at -Z (the prop's back), foot at +Z.  Side rails own the footprint.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * (half_w - 0.03), (rail_bottom + rail_top) * 0.5, 0.0),
               (0.06, rail_top - rail_bottom, size[2]), frame_uv, _tint(frame))
    for sz in (-1.0, 1.0):
        _solid(p, (0.0, (rail_bottom + rail_top) * 0.5, sz * (half_d - 0.03)),
               (size[0] - 0.12, rail_top - rail_bottom, 0.06), frame_uv,
               _tint(palette.shade(frame, 0.96)))
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            _solid(p, (sx * (half_w - 0.05), 0.04, sz * (half_d - 0.06)), (0.07, 0.08, 0.07),
                   board_uv, _tint(board, 0.5))
    # Headboard: posts to the catalogue height, a panel between them and a top
    # rail, so the bed has a clear head end from across the room.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * (half_w - 0.035), 0.275, -(half_d - 0.035)), (0.07, 0.55, 0.07),
               board_uv, _tint(board))
    _solid(p, (0.0, 0.40, -(half_d - 0.035)), (size[0] - 0.10, 0.24, 0.05), board_uv, _tint(board),
           hidden=("-y",))
    _solid(p, (0.0, 0.51, -(half_d - 0.035)), (size[0] - 0.06, 0.08, 0.07), board_uv,
           _tint(palette.shade(board, 1.06)), hidden=("-y",))
    # Footboard: lower than the head, so the bed reads head-to-foot at a glance.
    _solid(p, (0.0, 0.34, half_d - 0.025), (size[0] - 0.08, 0.20, 0.05), board_uv,
           _tint(palette.shade(board, 1.08)), hidden=("-y",))
    # Mattress slab plus a thin top layer: the piping read, no cloth sim.
    _solid(p, (0.0, (0.28 + mattress_top) * 0.5, -0.02), (size[0] - 0.10, mattress_top - 0.28, 1.76),
           mattress_uv, _tint(mattress))
    _solid(p, (0.0, (mattress_top + sheet_top) * 0.5, -0.02), (size[0] - 0.16, 0.03, 1.70),
           mattress_uv, _tint(palette.shade(mattress, 1.05)),
           colors={"+y": _tint(palette.shade(mattress, 1.08))})
    # Blanket over the foot two thirds, hanging over both sides and with a
    # heavier fold where it is turned back over the sheet.
    blanket_front, blanket_back = 0.88, -0.34
    _solid(p, (0.0, sheet_top + 0.03, (blanket_front + blanket_back) * 0.5),
           (size[0] - 0.02, 0.06, blanket_front - blanket_back), linen_uv, _tint(linen))
    for sx in (-1.0, 1.0):
        _solid(p, (sx * 0.635, 0.40, (blanket_front + blanket_back) * 0.5),
               (0.05, 0.13, blanket_front - blanket_back), linen_uv,
               _tint(palette.shade(linen, 0.94)))
    _solid(p, (0.0, 0.495, blanket_back), (size[0] - 0.02, 0.05, 0.11), linen_uv,
           _tint(palette.shade(linen, 1.1)), hidden=("-y",))
    # Two pillows against the headboard, each with a shallow puff on top.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * 0.30, (sheet_top + pillow_top) * 0.5, -0.70), (0.56, 0.08, 0.32),
               linen_uv, _tint(palette.shade(linen, 1.18)))
        _solid(p, (sx * 0.30, pillow_top + 0.008, -0.70), (0.48, 0.016, 0.26), linen_uv,
               _tint(palette.shade(linen, 1.24)), hidden=("-y",))
    p.add_note("raised headboard, draped blanket with side drops, two pillows")


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

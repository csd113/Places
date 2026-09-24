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
from pathlib import Path

import palette
from parts.refreshed import load_atlas, solid_box, solid_cylinder, padded_box
from mesh import FACE_KEYS, PropBuilder
from tex import decode_png

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

    Every hidden face is two triangles and two vertices of the triangle
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


def _build_seating(p: PropBuilder, seats: int) -> None:
    """Matching closed upholstered frames with bevelled loose cushions."""
    sofa = seats == 3
    tex = load_atlas(p, "couch" if sofa else "armchair", ("body", "seat", "back", "wood"))
    _split_cell(tex, "wood", "pillow")
    body_uv, seat_uv, back_uv, wood_uv, pillow_uv = (
        tex.uv(name, inset=1) for name in ("body", "seat", "back", "wood", "pillow"))
    width, _, depth = p.size
    white = (255, 255, 255)
    foot_x = 0.88 if sofa else 0.36
    for sx in (-1, 1):
        for sz in (-1, 1):
            solid_box(p, (sx * foot_x, 0.06, sz * 0.36), (0.06, 0.12, 0.06),
                      uv=wood_uv, color=white)
    solid_box(p, (0.0, 0.22, 0.0), (width, 0.20, depth), uv=body_uv, color=white)
    # Sink the back into the frame without sharing its rear edge/coplanar face.
    solid_box(p, (0.0, 0.599, -depth * 0.5 + 0.08), (width, 0.562, 0.158),
              uv=body_uv, color=white)
    padded_box(p, (0.0, 0.87, -depth * 0.5 + 0.095), (width, 0.06, 0.19),
               body_uv, bevel=0.018)
    arm_width, arm_top, radius = (0.17, 0.60, 0.085) if sofa else (0.15, 0.56, 0.07)
    arm_x = width * 0.5 - arm_width * 0.5
    for sx in (-1, 1):
        solid_box(p, (sx * arm_x, (0.32 + arm_top) * 0.5, 0.065),
                  (arm_width, arm_top - 0.32, 0.77), uv=body_uv, color=white)
        solid_cylinder(p, (sx * arm_x, arm_top, -0.34), radius, 0.76,
                       segments=12, axis="z", side_uv=body_uv, cap_uv=body_uv,
                       color=white, proxy=False)
    for index in range(seats):
        cx = (index - 1) * 0.56 if sofa else 0.0
        cushion_width = 0.535 if sofa else 0.58
        # One closed cushion replaces the old block plus inset top slab.
        seat_height = 0.17 if sofa else 0.175
        padded_box(p, (cx, 0.32 + seat_height * 0.5, 0.07),
                   (cushion_width, seat_height, 0.70), seat_uv, bevel=0.035)
        lean = (-6.0, -8.0, -5.0)[index] if sofa else -7.0
        padded_box(p, (cx, 0.64 if sofa else 0.62, -0.16),
                   (cushion_width, 0.36 if sofa else 0.32, 0.23 if sofa else 0.21),
                   back_uv, bevel=0.04, rotation=(lean, 0.0, 0.0))
    if sofa:
        for sx in (-1, 1):
            padded_box(p, (sx * 0.575, 0.62, -0.03), (0.32, 0.30, 0.13),
                       pillow_uv, bevel=0.035, rotation=(0.0, sx * 18.0, sx * -7.0))
    else:
        padded_box(p, (0.15, 0.60, -0.04), (0.30, 0.28, 0.12), pillow_uv,
                   bevel=0.035, rotation=(0.0, 24.0, -6.0))
    p.add_note("closed frame and 12-segment armrests; bevelled seat/back/throw cushions; file-backed upholstery")


def build_couch(p: PropBuilder) -> None:
    """Three-seat upholstered sofa, preserving the 2.0 x 0.9 x 0.9 m bounds."""
    _build_seating(p, seats=3)


def build_armchair(p: PropBuilder) -> None:
    """Matching armchair, preserving the 0.9 x 0.9 x 0.9 m bounds."""
    _build_seating(p, seats=1)


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
    """Closed timber components with outward winding and a file-backed oak atlas."""
    size = p.size  # [1.4, 0.75, 0.8]
    source = Path(__file__).resolve().parents[3] / "assets/core/props/models/table.png"
    width, height, pixels = decode_png(source.read_bytes())
    if (width, height) != (256, 256):
        raise ValueError("table.png must be the 256x256 four-region oak atlas")
    tex = p.set_texture(width, seed=71)
    tex.pixels[:] = pixels
    tex.auto("top", "wood", "edge", "apron")

    top_uv = tex.uv("top", inset=2)
    wood_uv = tex.uv("wood", inset=2)
    edge_uv = tex.uv("edge", inset=2)
    apron_uv = tex.uv("apron", inset=2)

    def outward(start: int, center) -> None:
        # Correct winding locally: shared primitives are used by other assets.
        mesh = p.mesh
        for offset in range(start, len(mesh.indices), 3):
            a, b, c = [mesh.positions[i] for i in mesh.indices[offset:offset + 3]]
            ab = [b[i] - a[i] for i in range(3)]
            ac = [c[i] - a[i] for i in range(3)]
            normal = (ab[1] * ac[2] - ab[2] * ac[1],
                      ab[2] * ac[0] - ab[0] * ac[2],
                      ab[0] * ac[1] - ab[1] * ac[0])
            if sum(normal[i] * (a[i] - center[i]) for i in range(3)) < 0:
                mesh.indices[offset + 1], mesh.indices[offset + 2] = (
                    mesh.indices[offset + 2], mesh.indices[offset + 1])

    def box(center, dimensions, **kwargs) -> None:
        start = len(p.mesh.indices)
        p.box(center, dimensions, **kwargs)
        outward(start, center)

    # The top slab owns the catalogue width and depth.
    box((0.0, 0.728, 0.0), (size[0], 0.044, size[2]),
        uv={"+y": top_uv, "-y": top_uv, "+x": edge_uv, "-x": edge_uv, "+z": edge_uv, "-z": edge_uv},
        color=(255, 255, 255))
    # The box top's default U axis runs across its depth; align oak along X.
    for index, (x, y, z) in enumerate(p.mesh.positions):
        if abs(y - 0.75) < 1e-6 and p.mesh.uvs[index][1] < 0.5:
            p.mesh.uvs[index] = (
                top_uv[0] + (x / size[0] + 0.5) * (top_uv[2] - top_uv[0]),
                top_uv[1] + (z / size[2] + 0.5) * (top_uv[3] - top_uv[1]))
    # An inset frame under the slab keeps the top from reading as a floating card.
    box((0.0, 0.688, 0.0), (size[0] - 0.10, 0.048, size[2] - 0.10),
        uv=apron_uv, color=(225, 225, 225))
    # Square legs, tapered to a slimmer foot the way a plain timber leg is.
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            base = (sx * (size[0] * 0.5 - 0.08), 0.0, sz * (size[2] * 0.5 - 0.07))
            start = len(p.mesh.indices)
            p.cylinder(base, 0.045 * 0.86, 0.706, segments=4, taper=1.0 / 0.86,
                       rotation=math.pi / 4, bottom=True,
                       side_uv=wood_uv, cap_uv=wood_uv, color=(255, 255, 255))
            outward(start, (base[0], 0.353, base[2]))
    for sz in (-1.0, 1.0):
        box((0.0, 0.638, sz * (size[2] * 0.5 - 0.085)), (size[0] - 0.20, 0.07, 0.03), uv=apron_uv,
            color=(255, 255, 255))
    for sx in (-1.0, 1.0):
        box((sx * (size[0] * 0.5 - 0.095), 0.638, 0.0), (0.03, 0.07, size[2] - 0.18), uv=apron_uv,
            color=(255, 255, 255))
    # Editor proxies have no texture, so give them the atlas's average oak tone.
    oak = tuple(sum(pixels[channel::4]) // (width * height) for channel in range(3))
    for part in p.mesh.parts:
        part["color"] = "#%02x%02x%02x" % oak
    p.add_note("closed slab and frame, four downward-tapered square legs, four aprons; file-backed oak atlas")


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
    tex = load_atlas(p, "bookshelf", ("side", "shelf", "back", "front"))
    body = shelf = back = front = (255, 255, 255)

    side_uv = tex.uv("side", inset=2)
    shelf_uv = tex.uv("shelf", inset=2)
    back_uv = tex.uv("back", inset=2)
    front_uv = tex.uv("front", inset=2)

    # Two full-height sides define the catalogue height and depth.
    for sx in (-1.0, 1.0):
        solid_box(p, (sx * (size[0] * 0.5 - 0.0175), size[1] * 0.5, 0.0), (0.035, size[1], size[2]),
              uv=side_uv, color=body)
    solid_box(p, (0.0, size[1] - 0.02, 0.0), (size[0] - 0.04, 0.04, size[2]), uv=front_uv, color=front)
    solid_box(p, (0.0, 0.02, 0.0), (size[0] - 0.04, 0.04, size[2]), uv=front_uv, color=front)
    # Hardboard back: the empty shelves need something dull behind them.
    solid_box(p, (0.0, size[1] * 0.5, -(size[2] * 0.5 - 0.01)), (size[0] - 0.04, size[1] - 0.04, 0.015),
          uv=back_uv, color=back)
    # Four shelves at five equal gaps, each with a front lip.
    gap = (size[1] - 0.22) / 5.0
    for index in range(4):
        cy = 0.04 + gap + 0.0175 + index * (0.035 + gap)
        solid_box(p, (0.0, cy, 0.01), (size[0] - 0.04, 0.035, size[2] - 0.05), uv=shelf_uv, color=shelf)
        solid_box(p, (0.0, cy, size[2] * 0.5 - 0.01), (size[0] - 0.04, 0.025, 0.02), uv=front_uv,
              color=front)
    # A top and a bottom rail stand in for a face frame (one texture seam each).
    solid_box(p, (0.0, size[1] - 0.06, size[2] * 0.5 - 0.01), (size[0] - 0.06, 0.04, 0.02), uv=front_uv,
          color=front)
    solid_box(p, (0.0, 0.03, size[2] * 0.5 - 0.01), (size[0] - 0.06, 0.04, 0.02), uv=front_uv,
          color=front)
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

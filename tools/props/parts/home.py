"""The Home theme's kitchen cabinetry: a shaker base cabinet and its wall unit.

The pack contract (see ``tools/props/README.md`` and the commented exemplar in
``parts/utility.py``) applies unchanged: metres, origin on the floor-contact
point, ``+Z`` facing the player, one 128x128 texture per prop, colours from
:mod:`palette` and the shared construction helpers from
:mod:`parts.furniture`.

These two props are the clean end of the pack -- an ordinary off-white shaker
kitchen rather than the faded institutional set.  There is deliberately no
rust, grime or wear: the only shading is a painted shaker recess, a whisper of
brush grain and the dark underside of the wall unit.  Panel seams are painted
on the texture, so each door leaf is a plain box; the carcass, the counter
slab, the recessed toe kick and the small bar handles carry the silhouette.
"""

from __future__ import annotations

import palette
from mesh import PropBuilder
from parts.furniture import _solid, _tint

# Triangle aims (the pack budget in ``build.py`` is the enforced one).
TARGET = {
    "home:cabinet_base": 90,
    "home:cabinet_wall": 70,
}

# The Home paint tone: the theme's off-white wall colour (#d8d3c8 / #ddd8ce),
# mixed from the shared palette so the cabinets sit in the same muted set as
# the rest of the pack instead of a showroom white.
PAINT = palette.mix(
    palette.mix(
        palette.hex_to_rgb(palette.PLASTIC_WHITE),
        palette.hex_to_rgb(palette.WALL_CREAM),
        0.30,
    ),
    palette.hex_to_rgb(palette.METAL_GREY),
    0.06,
)
# Restrained grey laminate for the base cabinet's counter slab.
COUNTER = palette.mix(
    palette.hex_to_rgb(palette.METAL_GREY),
    palette.hex_to_rgb(palette.METAL_SHADOW),
    0.45,
)
# Small brushed handles, one shade off the old chrome.
HANDLE = palette.mix(
    palette.hex_to_rgb(palette.CHROME),
    palette.hex_to_rgb(palette.PLASTIC_WHITE),
    0.35,
)
# The wall unit's underside: the paint dropped towards the shadow tone.
UNDER = palette.mix(PAINT, palette.hex_to_rgb(palette.METAL_SHADOW), 0.35)


# --------------------------------------------------------------------- paint


def _paint_cabinet(tex, region: str, base, seed: int) -> None:
    """Clean satin paint: flat, a faint brush grain, one soft edge line.

    Deliberately no grime or damage: the Home set is the tidy end of the pack,
    so the texture's job is the painted finish, not wear.
    """
    tex.fill(region, base, jitter=4, seed=seed)
    tex.noise(region, amount=3, freq=6, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.94), seed=seed + 2, density=0.14, alpha=14)
    tex.grain(region, palette.shade(base, 1.05), seed=seed + 3, density=0.12, alpha=12)
    tex.border(region, palette.shade(base, 0.90), width=1, alpha=26)


def _paint_door(tex, region: str, base, seed: int) -> None:
    """A shaker door: the recess is painted, never a modelled gap.

    ``Texture.panel`` strokes a highlight top edge and a shadow bottom edge
    around the recess, which is exactly the read a shaker frame needs; keeping
    it on the texture lets the door leaf stay a plain 16 mm box.
    """
    _paint_cabinet(tex, region, base, seed)
    tex.panel(region, palette.shade(base, 0.84), rect=(0.22, 0.09, 0.78, 0.91), depth=1, alpha=44)


def _paint_counter(tex, region: str, base, seed: int) -> None:
    """Grey laminate: fine speckle and a darker edge band, nothing glossy."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.noise(region, amount=4, freq=3, seed=seed + 1)
    tex.grain(region, palette.shade(base, 0.86), seed=seed + 2, density=0.22, alpha=20)
    tex.spots(region, palette.shade(base, 1.18), count=5, seed=seed + 3, radius=2, alpha=14)
    tex.spots(region, palette.shade(base, 0.80), count=5, seed=seed + 4, radius=2, alpha=14)
    tex.border(region, palette.shade(base, 0.70), width=1, alpha=70)


def _paint_metal(tex, region: str, base, seed: int) -> None:
    """Restrained brushed nickel for the handles: no rust, no grime."""
    tex.fill(region, base, jitter=5, seed=seed)
    tex.grain(region, palette.shade(base, 0.82), seed=seed + 1, density=0.30, alpha=30)
    tex.grain(region, palette.shade(base, 1.12), seed=seed + 2, density=0.24, alpha=24)
    tex.border(region, palette.shade(base, 0.88), width=1, alpha=40)


# ------------------------------------------------------------------ cabinets


def build_cabinet_base(p: PropBuilder) -> None:
    """Base cabinet: a carcass on a recessed toe kick, two shaker doors under
    a slim grey laminate counter that overhangs the doors.

    The counter slab owns the catalogue footprint; the carcass sits clear of
    the counter's back face and the doors hang proud of the carcass front so no
    two visible faces are coplanar.
    """
    width, height, depth = p.size  # [0.6, 0.9, 0.6]
    tex = p.set_texture(128, seed=131)
    tex.auto("body", "door", "counter", "metal")

    paint_fill = palette.shade(PAINT, 1.07)
    _paint_cabinet(tex, "body", paint_fill, seed=311)
    _paint_door(tex, "door", paint_fill, seed=317)
    _paint_counter(tex, "counter", palette.shade(COUNTER, 1.05), seed=323)
    _paint_metal(tex, "metal", palette.shade(HANDLE, 1.04), seed=331)

    body_uv = tex.uv("body")
    door_uv = tex.uv("door")
    counter_uv = tex.uv("counter")
    metal_uv = tex.uv("metal")

    body_tint = _tint(PAINT, 0.80)
    door_tint = _tint(PAINT, 0.90)
    kick_tint = palette.shade(PAINT, 0.52)
    counter_tint = _tint(COUNTER, 0.92)
    handle_tint = _tint(HANDLE, 0.45)

    counter_h = 0.035
    _solid(
        p,
        (0.0, height - counter_h * 0.5, 0.0),
        (width, counter_h, depth),
        counter_uv,
        counter_tint,
        hidden=("-y",),
        colors={"+y": _tint(COUNTER, 1.0)},
    )

    # Carcass: full width, its front recessed behind the doors and its top
    # sunk 7 mm into the counter so no faces are coplanar.
    carcass_bottom = 0.09
    carcass_top = height - counter_h + 0.007
    carcass_front, carcass_back = 0.266, -0.29
    _solid(
        p,
        (0.0, (carcass_bottom + carcass_top) * 0.5, (carcass_front + carcass_back) * 0.5),
        (width, carcass_top - carcass_bottom, carcass_front - carcass_back),
        body_uv,
        body_tint,
        hidden=("-y", "+y"),
    )

    # Toe kick: a plinth set back from the doors, read only in its own shadow.
    _solid(p, (0.0, 0.045, -0.037), (0.57, 0.09, 0.486), body_uv, kick_tint,
           hidden=("+y", "-y"))

    # Two shaker doors, each ~0.28 m wide with a 1.5 cm gap between the leaves
    # and a 1.25 cm reveal at the cabinet sides.  The front face has its own
    # texture region; the leaves are clear of the carcass by 2 mm.
    door_bottom, door_top = 0.10, 0.84
    door_w, door_depth, door_front = 0.28, 0.016, 0.284
    for sx in (-1.0, 1.0):
        _solid(
            p,
            (sx * (0.0075 + door_w * 0.5), (door_bottom + door_top) * 0.5, door_front - door_depth * 0.5),
            (door_w, door_top - door_bottom, door_depth),
            {"+z": door_uv, "-z": None, "+x": body_uv, "-x": body_uv, "+y": body_uv, "-y": body_uv},
            _tint(PAINT, 0.72),
            hidden=("-z",),
            colors={"+z": door_tint},
        )
    # Small vertical bar handles at the inner top corners of the doors.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * 0.045, 0.75, 0.292), (0.018, 0.12, 0.012), metal_uv, handle_tint,
               hidden=("-z",))
    p.add_note("recessed toe kick; painted shaker seams; counter overhangs the doors")


def build_cabinet_wall(p: PropBuilder) -> None:
    """Wall cabinet: a slim carcass with two shaker doors and small handles.

    No counter; the flat top is painted with the body, while the underside has
    its own dark region and tint -- real geometry, shadowed like the underside
    of a hung cabinet.
    """
    width, height, depth = p.size  # [0.6, 0.72, 0.33]
    tex = p.set_texture(128, seed=137)
    tex.auto("body", "door", "metal", "under")

    paint_fill = palette.shade(PAINT, 1.07)
    _paint_cabinet(tex, "body", paint_fill, seed=411)
    _paint_door(tex, "door", paint_fill, seed=417)
    _paint_metal(tex, "metal", palette.shade(HANDLE, 1.04), seed=423)
    _paint_cabinet(tex, "under", palette.shade(UNDER, 1.04), seed=431)

    body_uv = tex.uv("body")
    door_uv = tex.uv("door")
    metal_uv = tex.uv("metal")
    under_uv = tex.uv("under")

    body_tint = _tint(PAINT, 0.80)
    door_tint = _tint(PAINT, 0.90)
    handle_tint = _tint(HANDLE, 0.45)

    # Carcass: full catalogue width and height, back on the catalogue box.
    # The doors stand clear of its front by 2 mm.
    carcass_front, carcass_back = 0.132, -0.165
    _solid(
        p,
        (0.0, height * 0.5, (carcass_front + carcass_back) * 0.5),
        (width, height, carcass_front - carcass_back),
        {"+z": body_uv, "-z": body_uv, "+x": body_uv, "-x": body_uv, "+y": body_uv, "-y": under_uv},
        body_tint,
        colors={"+y": _tint(PAINT, 0.95), "-y": UNDER},
    )

    # Two doors with a 1.5 cm gap between the leaves and a reveal all round.
    door_bottom, door_top = 0.025, 0.695
    door_w, door_depth = 0.28, 0.016
    for sx in (-1.0, 1.0):
        _solid(
            p,
            (sx * (0.0075 + door_w * 0.5), (door_bottom + door_top) * 0.5, 0.142),
            (door_w, door_top - door_bottom, door_depth),
            {"+z": door_uv, "-z": None, "+x": body_uv, "-x": body_uv, "+y": body_uv, "-y": body_uv},
            _tint(PAINT, 0.72),
            hidden=("-z",),
            colors={"+z": door_tint},
        )
    # Handles at the bottom inner corners: a wall unit's doors lift open.
    for sx in (-1.0, 1.0):
        _solid(p, (sx * 0.045, 0.115, 0.159), (0.018, 0.10, 0.012), metal_uv, handle_tint,
               hidden=("-z",))
    p.add_note("painted shaker seams; flat top; dark textured underside")


PROPS = {
    "home:cabinet_base": build_cabinet_base,
    "home:cabinet_wall": build_cabinet_wall,
}

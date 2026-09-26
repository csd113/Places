"""The core pack's signage: a stop sign and a luminous exit sign.

Both follow the shared contract (metres, ``+Z`` facing the player, origin on
the floor-contact/bottom point, one texture per prop) and use the block font in
:mod:`glyphs` -- the same letterforms the external decal sheets draw -- so the
signage reads as one institutional set.

The stop sign is ordinary painted sheet metal with a single opaque material.
The exit sign is the pack's second luminous prop after the ball light: its
green face is a separate primitives/materials group with an ``emissiveFactor``
and ``KHR_materials_emissive_strength``, so the white EXIT lettering and arrow
glow while the dark housing stays nearly dark. The emissive term is
``emissiveFactor * base texture``, so the painted face doubles as the emission
mask.
"""

from __future__ import annotations

import math
from pathlib import Path

import palette
from glyphs import fill_polygon, stamp_text, text_width
from mesh import PropBuilder
from parts.furniture import _solid, _tint
from parts.refreshed import solid_cylinder, load_atlas_from, padded_box

# Triangle aims (the pack budget in ``build.py`` is the enforced one).
TARGET = {
    "core:stop_sign": 90,
    "core:exit_sign": 70,
}

WHITE = (247, 246, 241)
PLATE_RED = palette.hex_to_rgb("#a8453a")
EXIT_GREEN = palette.hex_to_rgb("#2f7a3f")


def _octagon(cx: float, cy: float, radius: float) -> list[tuple[float, float]]:
    """Flat-top octagon with a 0.45 m across-flats size."""
    return [
        (cx + radius * math.cos(math.tau * index / 8 + math.pi / 8) / math.cos(math.pi / 8), cy + radius * math.sin(math.tau * index / 8 + math.pi / 8) / math.cos(math.pi / 8))
        for index in range(8)
    ]


def _square_uv(x: float, y: float, width: float, rect) -> tuple[float, float]:
    """Maps a model-space point to a texture region addressed by its bbox square.

    ``width`` is the full square dimension (2R for the octagon) and ``rect`` a
    ``Texture.uv`` result already inset from the painted octagon.
    """
    u0, v0, u1, v1 = rect
    u = u0 + (0.5 + x / width) * (u1 - u0)
    v = v0 + (0.5 - y / width) * (v1 - v0)
    return (u, v)


# ------------------------------------------------------------------ stop sign


def build_stop_sign(p: PropBuilder) -> None:
    """Faded octagonal STOP plate on a galvanised pole, on two clamps.

    The octagon has horizontal top/bottom edges, so its 0.45 m
    across-flats span is exactly the catalogue width and height; its UVs map
    the plate's bounding square onto the painted octagon, so the mesh samples
    the paint one-to-one and the square's corners are never visible.
    """
    source = Path(__file__).resolve().parents[3] / "assets/core/props/models/stop_sign.png"
    tex = load_atlas_from(p, source, ("sign", "pole", "bracket", "back"))

    sign_uv = tex.uv("sign", inset=2)
    pole_uv = tex.uv("pole")
    bracket_uv = tex.uv("bracket")
    back_uv = tex.uv("back")

    sign_tint = _tint(PLATE_RED, 0.90)
    back_tint = _tint(palette.shade(PLATE_RED, 0.6), 0.55)
    pole_tint = _tint(palette.hex_to_rgb(palette.METAL_GREY), 0.42)
    bracket_tint = _tint(palette.hex_to_rgb(palette.METAL_DARK), 0.45)

    # Pole: r 0.025, foot on y = 0, stopping inside the plate.
    solid_cylinder(p, (0.0, 0.0, 0.0), 0.025, 1.55, segments=6, uv=pole_uv, color=pole_tint)

    # Octagon plate: flat-top eight-vertex ring, 0.035 m thick.
    radius = 0.225
    plate_y = 1.575
    front_z, back_z = 0.0175, -0.0175
    ring_front = [
        (radius * math.cos(math.tau * index / 8 + math.pi / 8) / math.cos(math.pi / 8), plate_y + radius * math.sin(math.tau * index / 8 + math.pi / 8) / math.cos(math.pi / 8), front_z)
        for index in range(8)
    ]
    ring_back = [(point[0], point[1], back_z) for point in ring_front]
    uv_front = [_square_uv(point[0], point[1] - plate_y, radius * 2.0, sign_uv) for point in ring_front]
    uv_back = [_square_uv(point[0], point[1] - plate_y, radius * 2.0, back_uv) for point in ring_back]
    centre_front = (0.0, plate_y, front_z)
    centre_back = (0.0, plate_y, back_z)
    middle_front_uv = ((sign_uv[0] + sign_uv[2]) * 0.5, (sign_uv[1] + sign_uv[3]) * 0.5)
    middle_back_uv = ((back_uv[0] + back_uv[2]) * 0.5, (back_uv[1] + back_uv[3]) * 0.5)
    for index in range(8):
        nxt = (index + 1) % 8
        p.mesh.triangle(centre_front, ring_front[index], ring_front[nxt],
                        [middle_front_uv, uv_front[index], uv_front[nxt]], sign_tint)
        p.mesh.triangle(centre_back, ring_back[nxt], ring_back[index],
                        [middle_back_uv, uv_back[index], uv_back[nxt]], back_tint)
        p.mesh.quad(ring_front[nxt], ring_front[index], ring_back[index], ring_back[nxt],
                    back_uv, back_tint)

    # Two clamps tying the plate to the pole; their depth owns the 0.06 m bbox.
    for clamp_y in (1.42, 1.68):
        _solid(p, (0.0, clamp_y, 0.0), (0.045, 0.022, 0.06), bracket_uv, bracket_tint, hidden=())

    p.add_note("flat-top octagon maps its bbox square onto the painted octagon; white STOP from the shared block font")
    p.add_note("one opaque material; the sign does not emit")


# ------------------------------------------------------------------ exit sign


def build_exit_sign(p: PropBuilder) -> None:
    """Luminous EXIT sign: a green face, dark housing, two rods and a canopy.

    The face is its own primitive/material group with an emissive factor, so
    the painted green panel, white lettering and white arrow glow while the
    housing stays dark. Oriented ``+Z``; the base (lowest geometry) is y = 0,
    so a level authors ``y = ceiling_y - 0.57``.
    """
    source = Path(__file__).resolve().parents[3] / "assets/core/props/models/exit_sign.png"
    tex = load_atlas_from(p, source, ())
    tex.region("face", (0, 0, 192, 144))
    tex.region("body", (192, 0, 64, 144))
    tex.region("metal", (0, 144, 128, 112))
    tex.region("lamp", (128, 144, 128, 112))
    housing = palette.mix(EXIT_GREEN, palette.hex_to_rgb(palette.ELECTRONICS_DARK), 0.62)

    face_uv = tex.uv("face")
    body_uv = tex.uv("body")
    metal_uv = tex.uv("metal")
    lamp_uv = tex.uv("lamp")

    body_tint = _tint(housing, 0.30)
    metal_tint = _tint(palette.hex_to_rgb(palette.METAL_GREY), 0.40)
    lamp_tint = _tint(palette.hex_to_rgb(palette.METAL_DARK), 0.40)

    body_slot = p.material("sign_body")
    face_slot = p.material("sign_face", emissive=(0.35, 1.0, 0.45), strength=1.5)

    p.begin_material(body_slot)
    # Closed bevelled housing behind an inset luminous face.
    padded_box(p, (0.0, 0.17, -0.002), (0.45, 0.34, 0.071), body_uv, bevel=0.009)
    # Two thin rods up to the canopy plate (4-segment cylinders: 16 tris each).
    for side in (-1.0, 1.0):
        solid_cylinder(p, (side * 0.12, 0.34, 0.0), 0.006, 0.205, segments=4, uv=metal_uv, color=metal_tint)
    # Canopy plate: 0.16 x 0.08, its underside its own darker region.
    _solid(
        p,
        (0.0, 0.555, 0.0),
        (0.16, 0.02, 0.08),
        {"+x": metal_uv, "-x": metal_uv, "+y": metal_uv, "-y": lamp_uv, "-z": metal_uv, "+z": metal_uv},
        metal_tint,
        hidden=(),
    )

    p.begin_material(face_slot)
    p.plane((0.0, 0.17, 0.0375), (0.426, 0.316, 0.0), normal="z", uv=face_uv, color=(255, 255, 255))

    p.add_note("face is its own emissive material group: emissiveFactor [0.35,1.0,0.45], strength 1.5")
    p.add_note("level owns the light: rect emitter 2 cm in front of the face (see the light-specialist report)")


PROPS = {
    "core:stop_sign": build_stop_sign,
    "core:exit_sign": build_exit_sign,
}

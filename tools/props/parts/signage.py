"""The core pack's signage: a stop sign and a luminous exit sign.

Both follow the shared contract (metres, ``+Z`` facing the player, origin on
the floor-contact/bottom point, one texture per prop) and load file-backed
atlases with bold, readable white lettering.

The stop sign is ordinary painted sheet metal with a single opaque material.
The exit sign is the pack's second luminous prop after the ball light: its
green face is a separate primitives/materials group with an ``emissiveFactor``
and ``KHR_materials_emissive_strength``, so the white EXIT lettering and arrow
glow while the painted metal housing remains non-emissive. The emissive term is
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
from parts.refreshed import solid_box, solid_cylinder, load_atlas_from, padded_box

# Triangle aims (the pack budget in ``build.py`` is the enforced one).
TARGET = {
    "core:stop_sign": 90,
    "core:exit_sign": 320,
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
    """Readable octagonal STOP plate mounted in front of a galvanised pole.

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

    # The PNG carries the finish: neutral vertex tints keep STOP and its
    # border white instead of multiplying them by the sign's red paint.
    sign_tint = (255, 255, 255)
    back_tint = (235, 235, 235)
    pole_tint = (245, 245, 245)
    bracket_tint = (235, 235, 235)

    # Pole spans z=-0.030..+0.010, entirely behind the plate's rear at
    # z=+0.024. It reaches both mounts without protruding over the plate.
    solid_cylinder(p, (0.0, 0.0, -0.010), 0.020, 1.73,
                   segments=8, uv=pole_uv, color=pole_tint)

    # A 6 mm metal plate, preserving the 0.45 x 1.8 x 0.06 m catalog bounds.
    radius = 0.225
    plate_y = 1.575
    front_z, back_z = 0.030, 0.024
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

    # Stand-off brackets meet the back of the plate, with a small overlap
    # into the post for a solid connection. Nothing crosses the sign face.
    for clamp_y in (1.43, 1.69):
        solid_box(p, (0.0, clamp_y, 0.0165), (0.034, 0.024, 0.015),
                  uv=bracket_uv, color=bracket_tint)

    p.add_note("clean file-backed STOP face; neutral metal back; rear pole with 14 mm plate clearance and two stand-offs")
    p.add_note("one opaque material; the sign does not emit")


# ------------------------------------------------------------------ exit sign


def build_exit_sign(p: PropBuilder) -> None:
    """Closed bevelled housing, recessed luminous panel and attached ceiling mount.

    +Z is the readable front. The full 0.45 x 0.57 x 0.08 m bounds and
    bottom-origin placement contract remain unchanged.
    """
    source = Path(__file__).resolve().parents[3] / "assets/core/props/models/exit_sign.png"
    tex = load_atlas_from(p, source, ())
    tex.region("face", (0, 0, 256, 128))
    tex.region("body", (0, 128, 128, 128))
    tex.region("metal", (128, 128, 128, 128))
    face_uv = tex.uv("face", inset=2)
    body_uv = tex.uv("body", inset=2)
    metal_uv = tex.uv("metal", inset=2)

    body_slot = p.material("sign_body")
    # Neutral emission keeps the white lettering white; the PNG supplies green.
    face_slot = p.material("sign_face", emissive=(1.0, 1.0, 1.0), strength=1.0)
    p.begin_material(body_slot)
    center_y = 0.1225

    def ring(width, height, corner, z):
        x, y = width * 0.5, height * 0.5
        return [(px, center_y + py, z) for px, py in (
            (-x + corner, -y), (x - corner, -y), (x, -y + corner),
            (x, y - corner), (x - corner, y), (-x + corner, y),
            (-x, y - corner), (-x, -y + corner))]

    # One continuous shell: back chamfer, side walls, front chamfer, bezel,
    # and recessed aperture. Shared ring positions avoid cracks and overlaps.
    rings = [
        ring(0.438, 0.233, 0.009, -0.040),
        ring(0.450, 0.245, 0.012, -0.034),
        ring(0.450, 0.245, 0.012, 0.034),
        ring(0.438, 0.233, 0.009, 0.040),
        ring(0.414, 0.207, 0.002, 0.040),
        ring(0.414, 0.207, 0.002, 0.034),
    ]
    for band, (back, front) in enumerate(zip(rings, rings[1:])):
        for index in range(8):
            nxt = (index + 1) % 8
            # Restrained baked shading on the opaque shell; a darker recess
            # makes the panel read as a separate inset piece at game distance.
            shade = 0.72 if band == 4 else (0.80, 0.87, 0.91, 0.97, 1.0, 0.93, 0.84, 0.77)[index]
            p.mesh.quad(back[index], back[nxt], front[nxt], front[index],
                        uv=body_uv, color=(255, 255, 255), shade_mult=shade)
    u0, v0, u1, v1 = body_uv
    for index in range(8):
        nxt = (index + 1) % 8
        p.mesh.triangle((0.0, center_y, -0.040), rings[0][nxt], rings[0][index],
                        [((u0 + u1) / 2, (v0 + v1) / 2), (u1, v1), (u0, v1)],
                        (225, 225, 225))

    # The canopy spans BOTH rods. Rod ends overlap their sockets inside the
    # housing and canopy, never floating beyond the mounting plate.
    for side in (-1.0, 1.0):
        x = side * 0.105
        solid_cylinder(p, (x, 0.239, 0.0), 0.005, 0.312, segments=8,
                       uv=metal_uv, color=(245, 245, 245))
        for y in (0.239, 0.536):
            solid_cylinder(p, (x, y, 0.0), 0.012, 0.014, segments=6,
                           uv=metal_uv, color=(245, 245, 245))
    padded_box(p, (0.0, 0.558, 0.0), (0.28, 0.024, 0.075), body_uv, bevel=0.004)

    # The lens closes the shell across a separate emissive material boundary.
    # Its 2:1 dimensions match the new top-half atlas region.
    p.begin_material(face_slot)
    u0, v0, u1, v1 = face_uv
    def panel_uv(point):
        return (u0 + (point[0] / 0.414 + 0.5) * (u1 - u0),
                v0 + (0.5 - (point[1] - center_y) / 0.207) * (v1 - v0))
    center = (0.0, center_y, 0.034)
    for index in range(8):
        nxt = (index + 1) % 8
        points = (center, rings[-1][index], rings[-1][nxt])
        p.mesh.triangle(*points, [panel_uv(point) for point in points], (255, 255, 255))

    p.add_note("closed chamfered metal housing with a 6 mm recessed 2:1 EXIT panel")
    p.add_note("two suspension rods with sockets, attached inside a full-width ceiling canopy")
    p.add_note("only the panel emits; level-authored lighting remains unchanged")


PROPS = {
    "core:stop_sign": build_stop_sign,
    "core:exit_sign": build_exit_sign,
}

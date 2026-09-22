"""The pack's exemplar prop: ``core:crate``.

Read this file first when adding a prop.  It demonstrates the whole contract:

* the build function receives a :class:`mesh.PropBuilder` and paints
  ``p.tex`` (a :class:`tex.Texture`) while pushing primitives into ``p.mesh``
  through the ``p.box`` / ``p.cylinder`` / ``p.plane`` helpers;
* texture regions are registered with ``t.auto(...)`` and referenced by name;
* the origin is the floor-contact centre, +Z is the front;
* geometry stays inside the catalogue ``size`` (checked by the generator);
* colours come from :mod:`palette` so the pack stays coherent.
"""

from __future__ import annotations

import palette
from mesh import PropBuilder
from tex import Texture

# Triangle budget for this prop (see tools/props/README.md).
BUDGET = 120


def build_crate(p: PropBuilder) -> None:
    size = p.size  # (width, height, depth) from assets/catalog.json
    tex = p.set_texture(64, seed=21)
    tex.auto("wood", "panel")

    # --- texture -----------------------------------------------------------
    wood = palette.hex_to_rgb(palette.CRATE_WOOD)
    tex.fill("wood", wood, jitter=10, seed=3)
    tex.grain("wood", palette.hex_to_rgb(palette.CRATE_DARK), seed=17, density=0.55, alpha=70)
    tex.streaks("wood", palette.hex_to_rgb(palette.GRIME), count=4, seed=5, alpha=26)
    tex.border("wood", palette.hex_to_rgb(palette.CRATE_DARK), width=1, alpha=70)

    panel = palette.hex_to_rgb(palette.CRATE_WOOD)
    tex.fill("panel", panel, jitter=8, seed=9)
    tex.grain("panel", palette.hex_to_rgb(palette.CRATE_DARK), seed=23, density=0.4, alpha=60)
    # Slats: the crate's bracing read comes from texture, not geometry.
    for index in range(3):
        v = 0.16 + index * 0.28
        tex.band("panel", palette.shade(panel, 0.78), v, v + 0.07, jitter=6)
        tex.band("panel", palette.shade(panel, 1.12), v + 0.07, v + 0.10, jitter=5)
    tex.spots("panel", palette.hex_to_rgb(palette.RUST), count=5, seed=13, radius=2, alpha=45)
    tex.border("panel", palette.hex_to_rgb(palette.CRATE_DARK), width=1, alpha=110)

    # --- geometry ----------------------------------------------------------
    # One box: the crate reads through its texture and proportions.  A slightly
    # larger "rim" box at the top gives the silhouette a bit of structure
    # without adding meaningful cost.
    plank = palette.hex_to_rgb(palette.CRATE_WOOD)
    dark = palette.shade(plank, 0.82)
    side_uv = tex.uv("panel")
    top_uv = tex.uv("panel")
    p.box(
        center=(0.0, size[1] * 0.5, 0.0),
        size=(size[0], size[1], size[2]),
        uv={
            "+x": side_uv,
            "-x": side_uv,
            "+z": tex.uv("wood"),
            "-z": tex.uv("wood"),
            "+y": top_uv,
            "-y": None,
        },
        colors={"+y": palette.shade(dark, 1.05)},
        color=plank,
    )

    # Corner battens: eight thin uprights keep the low-poly box from reading as
    # a plain cube at close range. They stay flush with the crate silhouette, so
    # the rendered size still matches the catalogue box exactly.
    batten = 0.03
    half_w = size[0] * 0.5
    half_d = size[2] * 0.5
    batten_color = palette.shade(plank, 0.88)
    for sx in (-1.0, 1.0):
        for sz in (-1.0, 1.0):
            p.box(
                center=(sx * (half_w - batten), size[1] * 0.5, sz * (half_d - batten)),
                size=(batten * 2.0, size[1], batten * 2.0),
                uv=tex.uv("wood"),
                color=batten_color,
                proxy=False,  # battens sit inside the crate silhouette
            )
    p.add_note("texture-driven slats; four corner battens for silhouette")


# Triangle budget for this prop (see tools/props/README.md).
BOX_BUDGET = 60


def build_cardboard_box(p: PropBuilder) -> None:
    """Cardboard box: a single box whose read is entirely painted.

    The crate next door spends triangles on corner battens; this prop spends
    none.  One 64x64 texture carries the whole story -- two flaps meeting under
    a strip of packing tape, scuffed edges, a faded shipping label and a
    stamped mark -- so the silhouette stays a plain cube exactly as the
    catalogue size describes.
    """
    size = p.size  # [0.5, 0.5, 0.5]
    tex = p.set_texture(64, seed=29)
    tex.auto("face", "side", "top")

    card = palette.hex_to_rgb(palette.CARDBOARD)
    dark = palette.hex_to_rgb(palette.CARDBOARD_DARK)
    tape = palette.hex_to_rgb(palette.CARDBOARD_TAPE)
    grime = palette.hex_to_rgb(palette.GRIME)
    paper = palette.hex_to_rgb(palette.PAPER)
    # The shader multiplies texture and vertex colour, so the vertex tint is
    # lifted towards the pack's light neutral (same trick as parts/furniture).
    tint = palette.mix(card, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.42)

    # --- texture: front and back faces (the tape wraps down from the top) ---
    tex.fill("face", card, jitter=9, seed=3)
    tex.noise("face", amount=5, freq=4, seed=4)
    tex.grain("face", dark, seed=5, density=0.32, alpha=42)
    tex.grain("face", palette.shade(card, 1.18), seed=6, density=0.22, alpha=24)
    # Tape down the middle of the face, ending in a torn, lifted edge.
    tex.bar("face", tape, (0.468, 0.0, 0.532, 0.18))
    tex.bar("face", palette.shade(tape, 1.10), (0.492, 0.0, 0.508, 0.18))
    tex.bar("face", palette.shade(tape, 0.86), (0.468, 0.15, 0.532, 0.18))
    # Shipping label, a stamped line and a faded directional mark.
    tex.bar("face", paper, (0.12, 0.60, 0.52, 0.80))
    tex.bar("face", palette.shade(paper, 0.88), (0.12, 0.60, 0.52, 0.635))
    tex.scribble("face", palette.shade(dark, 0.85), (0.15, 0.65, 0.49, 0.77), seed=7, text_blocks=2, alpha=170)
    tex.scribble("face", palette.shade(dark, 1.05), (0.60, 0.34, 0.92, 0.50), seed=8, text_blocks=3, alpha=80)
    tex.streaks("face", grime, count=3, seed=9, alpha=20)
    tex.spots("face", grime, count=3, seed=10, radius=2, alpha=24)
    tex.border("face", dark, width=2, alpha=100)

    # --- texture: left and right faces (no tape, one dented corner) ---------
    tex.fill("side", card, jitter=8, seed=13)
    tex.noise("side", amount=5, freq=4, seed=14)
    tex.grain("side", dark, seed=15, density=0.28, alpha=38)
    tex.streaks("side", palette.shade(dark, 0.95), count=2, seed=16, alpha=40)
    tex.bar("side", palette.shade(card, 0.92), (0.62, 0.0, 1.0, 0.20), alpha=110)
    tex.spots("side", grime, count=2, seed=17, radius=2, alpha=22)
    tex.border("side", dark, width=2, alpha=100)

    # --- texture: top face (two flaps under the taped seam) -----------------
    # The top face's region runs u along x (the seam is at u = 0.5) and v from
    # the front edge (v0) to the back (v1), so the tape is painted as a
    # vertical band and the flap step as two vertical halves.
    tex.fill("top", card, jitter=8, seed=21)
    tex.noise("top", amount=4, freq=4, seed=22)
    tex.grain("top", dark, seed=23, density=0.26, alpha=34)
    tex.bar("top", palette.shade(card, 0.94), (0.0, 0.0, 0.5, 1.0), alpha=120)
    tex.bar("top", palette.shade(card, 1.05), (0.5, 0.0, 1.0, 1.0), alpha=80)
    tex.bar("top", dark, (0.460, 0.0, 0.468, 1.0), alpha=150)
    tex.bar("top", dark, (0.532, 0.0, 0.540, 1.0), alpha=150)
    tex.bar("top", tape, (0.468, 0.0, 0.532, 1.0))
    tex.bar("top", palette.shade(tape, 1.10), (0.492, 0.0, 0.508, 1.0))
    tex.spots("top", grime, count=3, seed=24, radius=2, alpha=22)
    tex.border("top", dark, width=2, alpha=90)

    # --- geometry -----------------------------------------------------------
    face_uv = tex.uv("face")
    p.box(
        (0.0, size[1] * 0.5, 0.0),
        (size[0], size[1], size[2]),
        uv={
            "+z": face_uv,
            "-z": face_uv,
            "+x": tex.uv("side"),
            "-x": tex.uv("side"),
            "+y": tex.uv("top"),
            "-y": None,
        },
        color=tint,
        colors={"+y": palette.shade(tint, 1.05)},
    )
    p.add_note("single box; flaps, tape seam, scuffs and printed marks are paint only")


PROPS = {"core:crate": build_crate, "core:cardboard_box": build_cardboard_box}

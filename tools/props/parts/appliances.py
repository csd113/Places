"""Appliances props for the core pack: stove, sink, fridge, washing machine,
vending machine and water cooler.

The pack contract (see ``assets/README.md`` and the commented exemplar in
``parts/utility.py``) applies unchanged: metres, origin on the floor-contact
point, +Z facing the player, one 64x64/128x128 texture and one material per
prop, no alpha.  Every appliance here is a boxy silhouette plus a handful of
raised front panels -- doors, handles, portholes, control rails -- because at
480x272 the front elevation is what makes a prop recognisable and texture
pixels are cheaper than triangles.

Wear is applied with the same counts everywhere (see :func:`_wear`) so the six
read as one faded institutional set: brushed-metal grain, grime runs down the
flanks and a rust bloom in the lower third, where a mop would have reached.
"""

from __future__ import annotations

import math

import palette
from parts.refreshed import load_atlas, solid_box
from mesh import FACE_SHADE, PropBuilder

# Per-prop triangle aims from the pack brief.  They are targets, not limits:
# the pack budget is 50-500 preferred, 800 review, 1500 hard ceiling.
TARGET = {
    "core:stove": 230,
    "core:sink": 260,
    "core:fridge": 200,
    "core:washing_machine": 240,
    "core:vending_machine": 260,
    "core:water_cooler": 220,
    # Not placed by any level: the dynamic-object demonstrator (see
    # src/render/dynamic.rs).  It spins about its vertical axis on screen, so
    # the mouth, rim, basket wall and lifters have to read in motion.
    "core:washer_drum": 170,
}

# Shared finish constants: identical counts across all six appliances.
GRIME_STREAKS = 3
RUST_SPOTS = 3

# Texture-painting note for maintainers: the toolkit's ``dots``, ``bar`` and
# ``scribble`` write opaque pixels (their alpha keyword does not blend), while
# ``band``, ``border``, ``streaks``, ``spots`` and ``grain`` do.  Burners,
# knobs, buttons, labels and shelf lips below are therefore solid shapes by
# design, and the softly blended work goes through the band/spot helpers.


def _paint(tex, name, base, *, seed=0, grain_density=0.24, grain_alpha=22):
    """Faded painted metal: flat fill, low-frequency blotching, brushed dashes."""
    tex.fill(name, base, jitter=7, seed=seed + 1)
    tex.noise(name, amount=5, freq=3, seed=seed + 2)
    tex.grain(
        name,
        palette.hex_to_rgb(palette.METAL_DARK),
        seed=seed + 3,
        density=grain_density,
        alpha=grain_alpha,
    )


def _wear(tex, name, *, seed=0, streaks=GRIME_STREAKS, rust=RUST_SPOTS):
    """Grime runs plus a rust bloom confined to the lower third of the region.

    The region's bottom rows are the face's bottom rows on every side face, so
    the rust lands near the floor without any per-prop maths.
    """
    tex.streaks(name, palette.hex_to_rgb(palette.GRIME), count=streaks, seed=seed + 11, alpha=18)
    if rust:
        tex.spots(
            name,
            palette.hex_to_rgb(palette.RUST),
            count=rust,
            seed=seed + 12,
            radius=1,
            alpha=34,
            sub=(0.0, 0.68, 1.0, 1.0),
        )


def _porthole(p: PropBuilder, centre, radius: float, glass_radius: float, glass_z: float, front: float,
              segments: int, rim_uv, glass_uv, rim_color, glass_color) -> None:
    """Low-segment drum door: outer wall, front ring, inner wall, glass disc.

    ``centre`` is (x, y) on the machine's front; ``front`` is the z of the
    drum's outer ring and of the prop's front-most point; ``glass_z`` the
    recessed z of the glass.  Hand-built (rather than ``p.cylinder``) because
    the cylinder's caps all share three UVs and would smear the glass texture.
    """
    cx, cy = centre
    angles = [math.tau * index / segments for index in range(segments)]
    rim = [(cx + math.cos(a) * radius, cy + math.sin(a) * radius) for a in angles]
    inner = [(cx + math.cos(a) * glass_radius, cy + math.sin(a) * glass_radius) for a in angles]

    for index in range(segments):
        other = (index + 1) % segments
        # Outer wall of the drum, shaded like the toolkit's cylinders.
        p.mesh.quad(
            (rim[index][0], rim[index][1], glass_z),
            (rim[other][0], rim[other][1], glass_z),
            (rim[other][0], rim[other][1], front),
            (rim[index][0], rim[index][1], front),
            uv=rim_uv,
            color=rim_color,
            shade_mult=0.74 + 0.26 * (0.5 + 0.5 * math.cos(angles[index] - 0.9)),
        )
        # Front ring, proud of the glass.
        p.mesh.quad(
            (inner[index][0], inner[index][1], front),
            (rim[index][0], rim[index][1], front),
            (rim[other][0], rim[other][1], front),
            (inner[other][0], inner[other][1], front),
            uv=rim_uv,
            color=rim_color,
            shade_mult=FACE_SHADE["+z"],
        )
        # Inner wall, facing the drum's axis so the door reads as a recess.
        p.mesh.quad(
            (inner[other][0], inner[other][1], glass_z),
            (inner[index][0], inner[index][1], glass_z),
            (inner[index][0], inner[index][1], front),
            (inner[other][0], inner[other][1], front),
            uv=rim_uv,
            color=palette.shade(rim_color, 0.82),
        )

    # Glass: a fan whose vertices are mapped around the inscribed circle, so
    # the painted door graphics stay concentric instead of smearing.
    u0, v0, u1, v1 = glass_uv
    cu, cv = (u0 + u1) * 0.5, (v0 + v1) * 0.5
    ru, rv = (u1 - u0) * 0.5, (v1 - v0) * 0.5
    for index in range(segments):
        other = (index + 1) % segments
        p.mesh.triangle(
            (cx, cy, glass_z),
            (inner[index][0], inner[index][1], glass_z),
            (inner[other][0], inner[other][1], glass_z),
            [
                (cu, cv),
                (cu + ru * math.cos(angles[index]), cv - rv * math.sin(angles[index])),
                (cu + ru * math.cos(angles[other]), cv - rv * math.sin(angles[other])),
            ],
            glass_color,
            shade_mult=FACE_SHADE["+z"] * 0.8,
        )


# --------------------------------------------------------------------- stove


def build_stove(p: PropBuilder) -> None:
    """Freestanding range: cabinet, proud oven door, knob rail, painted hob."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = p.set_texture(64, seed=41)
    tex.auto("hob", "door", "panel", "side")

    metal = palette.shade(
        palette.mix(palette.hex_to_rgb(palette.METAL_GREY), palette.hex_to_rgb(palette.PLASTIC_BEIGE), 0.45), 0.88
    )
    metal_light = palette.shade(metal, 1.08)
    metal_dark = palette.shade(metal, 0.66)
    hob_face = palette.mix(palette.hex_to_rgb(palette.METAL_DARK), metal, 0.30)
    knob = palette.hex_to_rgb(palette.ELECTRONICS_DARK)
    glass = palette.hex_to_rgb(palette.SCREEN_DARK)

    # --- texture: hob ------------------------------------------------------
    # Burners are texture only.  The decal's own mapping puts the region's top
    # row at +Z, so the index marks painted at fy ~0.94 sit at the front edge.
    _paint(tex, "hob", metal_light, seed=1, grain_density=0.30)
    tex.bar("hob", palette.shade(hob_face, 0.96), (0.06, 0.06, 0.94, 0.94))
    tex.border("hob", metal_dark, width=1, alpha=120)
    for fx in (0.29, 0.71):
        for fy in (0.30, 0.70):
            tex.dots("hob", palette.shade(knob, 0.85), [(fx, fy)], radius=4, alpha=140)
            tex.dots("hob", palette.shade(hob_face, 1.25), [(fx, fy)], radius=2, alpha=210)
    for index in range(4):
        tex.dots("hob", palette.shade(knob, 1.05), [(0.13 + index * 0.24, 0.95)], radius=1, alpha=170)
    _wear(tex, "hob", seed=2, rust=2, streaks=2)

    # --- texture: oven door -------------------------------------------------
    _paint(tex, "door", palette.shade(metal, 0.97), seed=3)
    tex.border("door", metal_dark, width=1, alpha=120)
    tex.bar("door", palette.shade(metal_light, 1.04), (0.13, 0.11, 0.87, 0.63))
    tex.bar("door", glass, (0.17, 0.15, 0.83, 0.59))
    tex.band("door", palette.shade(glass, 1.7), 0.19, 0.27, alpha=55)
    tex.scribble("door", palette.shade(metal_dark, 0.85), (0.22, 0.70, 0.78, 0.88), seed=5, alpha=110)
    _wear(tex, "door", seed=4, rust=2)

    # --- texture: knob rail -------------------------------------------------
    _paint(tex, "panel", palette.shade(metal, 0.84), seed=6, grain_density=0.16, grain_alpha=16)
    tex.border("panel", palette.shade(metal_dark, 0.8), width=1, alpha=140)
    # Knob rings, aligned with the modelled knobs out in the geometry section.
    for fx in (0.10, 0.242, 0.383, 0.525):
        tex.dots("panel", palette.shade(knob, 0.8), [(fx, 0.50)], radius=3, alpha=120)
        tex.dots("panel", palette.shade(metal_light, 1.05), [(fx, 0.50)], radius=1, alpha=190)
    tex.bar("panel", glass, (0.62, 0.28, 0.92, 0.72))
    tex.band("panel", palette.shade(glass, 1.9), 0.32, 0.44, alpha=45)
    tex.scribble("panel", palette.shade(knob, 1.4), (0.64, 0.34, 0.90, 0.66), seed=7, text_blocks=2, alpha=120)

    # --- texture: flanks ----------------------------------------------------
    _paint(tex, "side", palette.shade(metal, 0.93), seed=8)
    tex.border("side", palette.shade(metal_dark, 0.95), width=1, alpha=70)
    tex.streaks("side", palette.shade(metal_dark, 0.9), count=2, seed=9, alpha=38)
    _wear(tex, "side", seed=10)

    # --- geometry -----------------------------------------------------------
    # Door plate plus handle sit 3.5 cm proud of the cabinet so the bounding
    # box lands exactly on the catalogue depth.
    proud = 0.035
    cabinet_front = half_d - proud
    cabinet_depth = depth - proud
    plinth_h = 0.05
    rail_h = 0.14
    rail_bottom = height - rail_h
    hob_z = (cabinet_front - half_d) * 0.5  # centre of the (recessed) body slab

    plinth_color = palette.shade(metal_dark, 0.66)
    p.box(
        (0.0, plinth_h * 0.5, hob_z),
        (width - 0.06, plinth_h, cabinet_depth - 0.05),
        uv=tex.uv("side"),
        color=plinth_color,
        proxy=False,  # the plinth sits inside the body silhouette
    )
    p.box(
        (0.0, plinth_h + (rail_bottom - plinth_h) * 0.5, hob_z),
        (width, rail_bottom - plinth_h, cabinet_depth),
        uv={"+z": tex.uv("side"), "-z": tex.uv("side"), "+y": tex.uv("side"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=metal,
    )
    # Oven door: a raised plate, then the handle across its top.
    door_bottom, door_top = 0.14, 0.66
    p.box(
        (0.0, (door_bottom + door_top) * 0.5, cabinet_front + 0.01),
        (width - 0.06, door_top - door_bottom, 0.02),
        uv={"+z": tex.uv("door"), "-z": None, "+y": tex.uv("side"), "-y": None,
            "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=metal_light,
    )
    p.box(
        (0.0, door_top - 0.035, cabinet_front + 0.0275),
        (width - 0.08, 0.03, 0.015),
        uv=tex.uv("side"),
        color=palette.hex_to_rgb(palette.CHROME),
        proxy=False,
    )
    # Drawer seam under the door.
    p.box(
        (0.0, 0.095, cabinet_front + 0.01),
        (width - 0.06, 0.07, 0.02),
        uv=tex.uv("side"),
        color=palette.shade(metal, 1.02),
        proxy=False,
    )
    # Knob rail: the slab under the hob carries the controls and four knobs.
    p.box(
        (0.0, rail_bottom + (height - 0.0025 - rail_bottom) * 0.5, hob_z),
        (width, height - 0.0025 - rail_bottom, cabinet_depth),
        uv={"+z": tex.uv("panel"), "-z": tex.uv("side"), "+y": tex.uv("hob"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=palette.shade(metal, 0.88),
    )
    for index in range(4):
        p.cylinder(
            (-0.24 + index * 0.085, 0.829, cabinet_front),
            0.018,
            0.016,
            segments=5,
            axis="z",
            side_uv=tex.uv("side"),
            cap_uv=tex.uv("side"),
            color=knob,
            proxy=False,
        )
    # Hob decal 2.5 mm above the slab: 2 triangles buy the burner layout with
    # the intuitive plan mapping instead of the box top's rotated UVs.
    plane = width - 0.08
    p.plane((0.0, height, hob_z), (plane, 0.0, plane), uv=tex.uv("hob"),
            color=palette.shade(hob_face, 1.02), shade_mult=FACE_SHADE["+y"])
    p.add_note("burners and oven window are texture; raised door, handle and knob rail carry the silhouette")


# ---------------------------------------------------------------------- sink


def build_sink(p: PropBuilder) -> None:
    """Kitchen sink unit: cabinet with two doors, counter with a painted basin."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = p.set_texture(64, seed=47)
    tex.auto("door", "side", "basin", "frame")

    steel = palette.mix(palette.hex_to_rgb(palette.METAL_LIGHT), palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL), 0.16)
    steel_mid = palette.shade(steel, 0.86)
    steel_dark = palette.shade(steel, 0.6)
    basin_floor = palette.mix(palette.hex_to_rgb(palette.METAL_DARK), steel, 0.42)
    chrome = palette.hex_to_rgb(palette.CHROME)

    # --- texture: cabinet door ---------------------------------------------
    _paint(tex, "door", palette.shade(steel_mid, 1.02), seed=1)
    tex.panel("door", steel_dark, rect=(0.08, 0.05, 0.92, 0.95), depth=1, alpha=70)
    tex.border("door", palette.shade(steel_dark, 0.8), width=1, alpha=110)
    _wear(tex, "door", seed=2)

    # --- texture: flanks and cabinet frame ---------------------------------
    _paint(tex, "side", palette.shade(steel_mid, 0.94), seed=3)
    tex.border("side", steel_dark, width=1, alpha=70)
    _wear(tex, "side", seed=4)
    _paint(tex, "frame", palette.shade(steel_mid, 0.84), seed=5, grain_density=0.18)
    tex.border("frame", steel_dark, width=1, alpha=90)
    _wear(tex, "frame", seed=6)

    # --- texture: counter with basin recess ---------------------------------
    # The counter top's +Y face maps the region rotated a quarter turn (region
    # rows run along x, columns along z), so the basin is painted symmetric.
    _paint(tex, "basin", palette.shade(steel, 1.05), seed=7, grain_density=0.3)
    tex.border("basin", steel_dark, width=1, alpha=90)
    tex.bar("basin", palette.shade(steel_mid, 0.88), (0.16, 0.16, 0.84, 0.84))
    tex.bar("basin", basin_floor, (0.20, 0.20, 0.80, 0.80))
    tex.band("basin", palette.shade(basin_floor, 1.35), 0.23, 0.28, alpha=80)
    tex.dots("basin", palette.shade(steel_dark, 0.6), [(0.5, 0.5)], radius=3, alpha=170)
    tex.dots("basin", palette.shade(chrome, 1.1), [(0.5, 0.5)], radius=1, alpha=200)
    _wear(tex, "basin", seed=8, rust=1, streaks=2)

    # --- geometry -----------------------------------------------------------
    proud = 0.035
    cabinet_front = half_d - proud
    cabinet_depth = depth - proud
    cabinet_top = 0.66            # counter slab sits on the cabinet carcass
    counter_h = 0.04
    counter_top = cabinet_top + counter_h

    p.box(
        (0.0, 0.03, -0.02),
        (width - 0.06, 0.06, cabinet_depth - 0.06),
        uv=tex.uv("frame"),
        color=palette.shade(steel_dark, 0.75),
        proxy=False,
    )
    p.box(
        (0.0, 0.06 + (cabinet_top - 0.06) * 0.5, -0.0175),
        (width, cabinet_top - 0.06, cabinet_depth),
        uv={"+z": tex.uv("frame"), "-z": tex.uv("side"), "+y": tex.uv("side"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=steel_mid,
    )
    for sx in (-1.0, 1.0):
        p.box(
            (sx * 0.1525, 0.10 + (0.60 - 0.10) * 0.5, cabinet_front + 0.01),
            (0.275, 0.60 - 0.10, 0.02),
            uv={"+z": tex.uv("door"), "-z": None, "+y": tex.uv("side"), "-y": None,
                "+x": tex.uv("side"), "-x": tex.uv("side")},
            color=palette.shade(steel_mid, 1.04),
        )
        p.box(
            (sx * 0.075, 0.35, cabinet_front + 0.0275),
            (0.024, 0.13, 0.015),
            uv=tex.uv("side"),
            color=chrome,
            proxy=False,
        )
    # Counter slab with a raised back edge: the upstand is what makes a painted
    # basin read as a sink unit rather than a cabinet with a lid.
    p.box(
        (0.0, cabinet_top + counter_h * 0.5, 0.0),
        (width, counter_h, depth),
        uv={"+z": tex.uv("side"), "-z": tex.uv("side"), "+y": tex.uv("basin"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=steel,
    )
    p.box(
        (0.0, counter_top + 0.035, -half_d + 0.0125),
        (width, 0.07, 0.025),
        uv={"+z": tex.uv("side"), "-z": tex.uv("side"), "+y": tex.uv("side"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=palette.shade(steel, 1.03),
    )
    # Faucet: a taller post, a forward spout and a cross handle.  Its tip is the
    # prop's highest point, just inside the 0.85 m catalogue height.
    p.cylinder((0.0, counter_top, -0.17), 0.02, 0.145, segments=6, side_uv=tex.uv("side"),
               cap_uv=tex.uv("side"), color=chrome)
    p.tube((0.0, 0.838, -0.17), (0.0, 0.808, -0.05), 0.016, segments=6, uv=tex.uv("side"),
           color=chrome)
    p.cylinder((-0.04, 0.795, -0.17), 0.009, 0.08, segments=4, axis="x",
               side_uv=tex.uv("side"), cap_uv=tex.uv("side"), color=chrome, proxy=False)
    p.add_note("basin recess is painted on the counter top; upstand and faucet are geometry")


# -------------------------------------------------------------------- fridge


def build_fridge(p: PropBuilder) -> None:
    """Tall fridge: one body, two proud doors and two thin vertical handles."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = load_atlas(p, "fridge", ("freezer", "fridge", "side", "top"))
    metal = metal_light = (255, 255, 255)
    metal_dark = (120, 120, 120)

    # --- geometry -----------------------------------------------------------
    proud = 0.04  # door (2 cm) + handle (2 cm)
    body_front = half_d - proud
    plinth_h = 0.06
    seam = 1.185  # freezer / fridge door split

    solid_box(p,
        (0.0, plinth_h * 0.5, -0.02),
        (width - 0.04, plinth_h, depth - 0.08),
        uv=tex.uv("side", inset=2),
        color=palette.shade(metal_dark, 0.85),
        proxy=False,
    )
    solid_box(p,
        (0.0, plinth_h + (height - plinth_h) * 0.5, (body_front - half_d) * 0.5),
        (width, height - plinth_h, depth - proud),
        uv={"+z": tex.uv("side", inset=2), "-z": tex.uv("side", inset=2), "+y": tex.uv("top", inset=2),
            "-y": None, "+x": tex.uv("side", inset=2), "-x": tex.uv("side", inset=2)},
        color=metal,
    )
    doors = (("freezer", seam + 0.015, height - 0.01), ("fridge", 0.07, seam - 0.015))
    for name, bottom, top in doors:
        solid_box(p,
            (0.0, (bottom + top) * 0.5, body_front + 0.01),
            (width - 0.04, top - bottom, 0.02),
            uv={"+z": tex.uv(name), "-z": None, "+y": tex.uv("side", inset=2), "-y": None,
                "+x": tex.uv("side", inset=2), "-x": tex.uv("side", inset=2)},
            color=metal_light,
        )
    # Two thin vertical handles, aligned across the door seam.  They stay a
    # mid grey so they read against the pale doors.
    handle = palette.mix(metal, palette.hex_to_rgb(palette.METAL_DARK), 0.55)
    for bottom, top in ((1.24, 1.50), (0.87, 1.13)):
        solid_box(p,
            (0.25, (bottom + top) * 0.5, body_front + 0.03),
            (0.035, top - bottom, 0.02),
            uv=tex.uv("side", inset=2),
            color=handle,
        )
    # Hinges on the opposite side, visible in profile.
    for axis_y in (1.76, 1.19, 0.09):
        solid_box(p,
            (-0.33, axis_y, body_front + 0.025),
            (0.04, 0.07, 0.025),
            uv=tex.uv("side", inset=2),
            color=palette.shade(metal, 0.7),
            proxy=False,
        )
    p.add_note("freezer/fridge split is a modelled door seam plus painted gaskets; handles are 2 cm bars")


# ----------------------------------------------------------- washing machine


def build_washing_machine(p: PropBuilder) -> None:
    """Front loader: box body, 8-segment porthole with a recessed glass, rail."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = p.set_texture(128, seed=59)
    tex.auto("front", "strip", "rim", "glass")

    metal = palette.shade(
        palette.mix(palette.hex_to_rgb(palette.METAL_GREY), palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.30), 0.95
    )
    metal_light = palette.shade(metal, 1.06)
    metal_dark = palette.shade(metal, 0.62)
    rim_color = palette.shade(metal, 0.8)
    glass = palette.shade(palette.hex_to_rgb(palette.GLASS_TINT), 0.85)
    dark = palette.hex_to_rgb(palette.ELECTRONICS_DARK)

    # --- texture: body front (visible as a ring around the drum) -----------
    _paint(tex, "front", palette.shade(metal, 0.96), seed=1, grain_density=0.18, grain_alpha=16)
    tex.border("front", metal_dark, width=1, alpha=60)
    _wear(tex, "front", seed=2)

    # --- texture: control strip --------------------------------------------
    _paint(tex, "strip", palette.shade(metal, 0.88), seed=3, grain_density=0.14, grain_alpha=14)
    tex.border("strip", metal_dark, width=1, alpha=120)
    tex.bar("strip", dark, (0.62, 0.22, 0.95, 0.78))
    for index in range(3):
        fx = 0.08 + index * 0.16
        tex.bar("strip", palette.shade(metal_light, 1.05), (fx, 0.20, fx + 0.05, 0.80))
        tex.bar("strip", palette.shade(dark, 1.3), (fx + 0.015, 0.32, fx + 0.035, 0.68))
    for index, fx in enumerate((0.30, 0.36, 0.42, 0.48)):
        tex.dots("strip", palette.hex_to_rgb(palette.FABRIC_RED if index else palette.INSTITUTIONAL_GREEN),
                 [(fx, 0.36)], radius=1, alpha=170)

    # --- texture: drum rim / body trim --------------------------------------
    _paint(tex, "rim", rim_color, seed=4, grain_density=0.3, grain_alpha=20)
    _wear(tex, "rim", seed=5, rust=1, streaks=2)

    # --- texture: porthole glass (mapped as a disc) --------------------------
    # Concentric solid discs: the rim catches light, the middle is the drum
    # mouth.  Texels are painted lighter than the vertex colour on purpose --
    # the shader multiplies the two, so a "dark glass" albedo ends up black.
    glass_dark = palette.hex_to_rgb(palette.SCREEN_DARK)
    glass_light = palette.hex_to_rgb(palette.GLASS_TINT)
    glass_mid = palette.mix(glass_dark, glass_light, 0.55)
    tex.fill("glass", glass_mid, jitter=6, seed=6)
    tex.dots("glass", glass_light, [(0.5, 0.5)], radius=30)
    tex.dots("glass", glass_mid, [(0.5, 0.5)], radius=25)
    tex.dots("glass", glass_dark, [(0.5, 0.52)], radius=20)
    tex.band("glass", glass_dark, 0.28, 0.4, alpha=50)
    tex.spots("glass", glass_light, count=3, seed=7, radius=3, alpha=60)

    # --- geometry -----------------------------------------------------------
    drum_radius = 0.215
    drum_front = half_d
    body_front = drum_front - 0.03
    body = depth - 0.03
    body_top = height - 0.05
    centre_y = 0.44

    p.box(
        (0.0, 0.02, -0.025),
        (width - 0.06, 0.04, body - 0.06),
        uv=tex.uv("rim"),
        color=palette.shade(metal_dark, 0.8),
        proxy=False,
    )
    p.box(
        (0.0, 0.04 + (body_top - 0.04) * 0.5, (body_front - half_d) * 0.5),
        (width, body_top - 0.04, body),
        uv={"+z": tex.uv("front"), "-z": tex.uv("rim"), "+y": tex.uv("front"),
            "-y": None, "+x": tex.uv("front"), "-x": tex.uv("front")},
        color=metal,
    )
    p.box(
        (0.0, body_top + 0.025, (body_front - half_d) * 0.5 + 0.005),
        (width - 0.03, height - body_top, body - 0.05),
        uv={"+z": tex.uv("front"), "-z": tex.uv("rim"), "+y": tex.uv("front"),
            "-y": None, "+x": tex.uv("front"), "-x": tex.uv("front")},
        color=metal_light,
    )
    # The glass must sit in front of the body's front face, or the opaque body
    # panel would occlude the whole porthole.
    _porthole(p, (0.0, centre_y), drum_radius, 0.155, body_front + 0.012, drum_front, 8,
              tex.uv("rim"), tex.uv("glass"), rim_color, glass)
    # Control rail above the drum, with a detergent drawer and one dial.
    p.box(
        (0.0, 0.73, body_front + 0.01),
        (width - 0.06, 0.13, 0.02),
        uv={"+z": tex.uv("strip"), "-z": None, "+y": tex.uv("rim"), "-y": None,
            "+x": tex.uv("rim"), "-x": tex.uv("rim")},
        color=palette.shade(metal, 0.9),
    )
    p.box(
        (-0.13, 0.73, body_front + 0.025),
        (0.15, 0.09, 0.01),
        uv=tex.uv("rim"),
        color=metal_light,
        proxy=False,
    )
    p.cylinder((0.19, 0.73, body_front + 0.015), 0.022, 0.015, segments=6, axis="z",
               side_uv=tex.uv("rim"), cap_uv=tex.uv("rim"), color=dark)
    p.add_note("8-segment porthole with a recessed, radially mapped glass; control rail with drawer and dial")


# --------------------------------------------------------------- washer drum


def build_washer_drum(p: PropBuilder) -> None:
    """Open drum basket: shell, rim, inner wall, floor and three lifters.

    The demonstrator for the dynamic-object path (`src/render/dynamic.rs`):
    a level spawns it in front of a placed washing machine and turns it about
    its own vertical axis.  Because it is seen in motion from standing height,
    the paint budget goes on the parts that move -- the mouth's rim, the inner
    wall and the drum floor -- and on three lifters, which are what make the
    rotation legible at all.  It stands on its base like a basket lifted out of
    the machine, so its local Y axis is the spin axis the transform applies.
    """
    width, height, _depth = p.size
    radius = width * 0.5
    rim_w = 0.045
    mouth_radius = radius - rim_w
    drum_floor = 0.05
    rim_bottom = height - 0.04
    segments = 12
    lifter_angles = (30.0, 150.0, 270.0)

    tex = p.set_texture(64, seed=73)
    tex.auto("shell", "rim", "facing", "lifter")

    steel = palette.shade(
        palette.mix(palette.hex_to_rgb(palette.METAL_LIGHT),
                    palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL), 0.10), 0.96
    )
    steel_light = palette.shade(steel, 1.10)
    steel_dark = palette.shade(steel, 0.58)
    inside = palette.shade(
        palette.mix(palette.hex_to_rgb(palette.METAL_DARK),
                    palette.hex_to_rgb(palette.SCREEN_DARK), 0.35), 0.92
    )
    chrome = palette.hex_to_rgb(palette.CHROME)

    # --- texture: outer shell ----------------------------------------------
    _paint(tex, "shell", steel, seed=1, grain_density=0.22)
    # One seam per segment: the basket's pressed panels.
    for index in range(segments):
        u = index / segments
        tex.bar("shell", steel_dark, (max(0.0, u - 0.012), 0.0, min(1.0, u + 0.012), 1.0), alpha=90)
    # Two rows of perforation dots read as the basket's wall in the light.
    for row in (0.58, 0.70):
        tex.dots("shell", palette.shade(steel_dark, 0.72),
                 [(index / (segments * 2) + 0.02, row) for index in range(segments * 2)],
                 radius=1, alpha=120)
    # One asymmetrically painted service panel, on the +Z world side at spawn:
    # the drum turns about its own axis, so a single marked segment is what
    # makes the rotation legible from outside; the rest of the shell's detail
    # is 12-fold symmetric. u = 0.25 is the model's front.
    tex.bar("shell", palette.shade(steel, 0.72), (0.222, 0.06, 0.278, 0.94), alpha=150)
    tex.scribble("shell", palette.shade(steel_dark, 0.7), (0.228, 0.30, 0.272, 0.46),
                 seed=8, text_blocks=1, alpha=170)
    tex.border("shell", steel_dark, width=1, alpha=70)
    _wear(tex, "shell", seed=2, rust=2, streaks=2)

    # --- texture: rim -------------------------------------------------------
    _paint(tex, "rim", chrome, seed=3, grain_density=0.4, grain_alpha=26)
    tex.band("rim", palette.shade(chrome, 0.78), 0.0, 0.30, alpha=60)
    tex.spots("rim", palette.hex_to_rgb(palette.GRIME), count=3, seed=4, radius=2, alpha=30)

    # --- texture: drum floor (mapped as a disc, centre in the middle) -------
    _paint(tex, "facing", inside, seed=5, grain_density=0.3, grain_alpha=24)
    for ring in (0.16, 0.31, 0.46):
        tex.band("facing", palette.shade(inside, 0.80), ring, ring + 0.035, alpha=70)
    tex.dots("facing", palette.shade(inside, 0.55), [(0.5, 0.5)], radius=3)
    tex.dots("facing", palette.shade(steel_dark, 0.9), [(0.5, 0.5)], radius=1, alpha=200)
    tex.spots("facing", palette.hex_to_rgb(palette.GRIME), count=4, seed=6, radius=2, alpha=34)

    # --- texture: lifters ---------------------------------------------------
    _paint(tex, "lifter", steel_light, seed=7, grain_density=0.3)
    tex.band("lifter", palette.shade(steel_dark, 0.9), 0.55, 0.70, alpha=60)
    tex.border("lifter", steel_dark, width=1, alpha=80)

    # --- geometry -----------------------------------------------------------
    def ring(angle: float, r: float, y: float):
        return (math.cos(angle) * r, y, math.sin(angle) * r)

    def angle_of(index: int) -> float:
        return (index / segments) * math.tau

    def sector_shade(index: int) -> float:
        return 0.72 + 0.28 * (0.5 + 0.5 * math.cos(angle_of(index) - 0.9))

    shell_uv = tex.uv("shell")
    rim_uv = tex.uv("rim")
    facing_uv = tex.uv("facing")
    lifter_uv = tex.uv("lifter")
    u0, v0, u1, v1 = shell_uv
    facing_u0, facing_v0, facing_u1, facing_v1 = facing_uv
    facing_cu = (facing_u0 + facing_u1) * 0.5
    facing_cv = (facing_v0 + facing_v1) * 0.5
    facing_ru = (facing_u1 - facing_u0) * 0.5
    facing_rv = (facing_v1 - facing_v0) * 0.5
    for index in range(segments):
        nxt = (index + 1) % segments
        a0, a1 = angle_of(index), angle_of(nxt)
        t0, t1 = index / segments, nxt / segments
        multiplier = sector_shade(index)
        # Outer shell, open at the bottom.
        p.mesh.quad(
            ring(a0, radius, 0.0),
            ring(a1, radius, 0.0),
            ring(a1, radius, rim_bottom),
            ring(a0, radius, rim_bottom),
            [(u0 + (u1 - u0) * t0, v1), (u0 + (u1 - u0) * t1, v1),
             (u0 + (u1 - u0) * t1, v0), (u0 + (u1 - u0) * t0, v0)],
            steel,
            shade_mult=multiplier,
        )
        # Bottom annulus, so the basket is closed from below.
        p.mesh.quad(
            ring(a0, mouth_radius, 0.0),
            ring(a1, mouth_radius, 0.0),
            ring(a1, radius, 0.0),
            ring(a0, radius, 0.0),
            rim_uv,
            palette.shade(steel_dark, 0.8),
            shade_mult=0.62,
        )
        # Polished rim between the shell and the mouth.
        p.mesh.quad(
            ring(a0, radius, height),
            ring(a1, radius, height),
            ring(a1, mouth_radius, height),
            ring(a0, mouth_radius, height),
            rim_uv,
            chrome,
            shade_mult=1.0,
        )
        # Inner basket wall, from the mouth down to the drum floor.
        p.mesh.quad(
            ring(a0, mouth_radius, drum_floor),
            ring(a1, mouth_radius, drum_floor),
            ring(a1, mouth_radius, height),
            ring(a0, mouth_radius, height),
            facing_uv,
            palette.shade(steel, 0.86),
            shade_mult=0.80 + 0.14 * (0.5 + 0.5 * math.cos(angle_of(index) + 1.6)),
        )
        # Drum floor: a fan whose UVs run around the painted disc, so the
        # concentric wear stays concentric instead of smearing from one corner.
        p.mesh.triangle(
            (0.0, drum_floor, 0.0),
            ring(a0, mouth_radius, drum_floor),
            ring(a1, mouth_radius, drum_floor),
            [
                (facing_cu, facing_cv),
                (facing_cu + facing_ru * math.cos(a0), facing_cv - facing_rv * math.sin(a0)),
                (facing_cu + facing_ru * math.cos(a1), facing_cv - facing_rv * math.sin(a1)),
            ],
            palette.shade(inside, 1.05),
            shade_mult=0.94,
        )

    # Three lifters: the paddles that make a turning drum read as a drum.
    lifter_half = 0.05
    lifter_radius = mouth_radius - 0.02
    lifter_mid = drum_floor + 0.085
    for angle in lifter_angles:
        radians = math.radians(angle)
        p.box(
            (math.cos(radians) * lifter_radius, lifter_mid, math.sin(radians) * lifter_radius),
            (lifter_half * 2.0, 0.15, 0.03),
            uv=lifter_uv,
            color=steel_light,
            rotation=(0.0, 90.0 - angle, 0.0),
            proxy=False,  # internal: the shell proxy already covers the silhouette
        )
    # The editor's derived preview only needs the outer mass.
    p.mesh.parts.append(
        {
            "shape": "cylinder",
            "axis": "y",
            "base": [0.0, 0.0, 0.0],
            "radius": round(radius, 4),
            "height": round(height, 4),
            "segments": segments,
            "taper": 1.0,
            "color": "#b0b4b6",
        }
    )
    p.add_note("open basket: rim, inner wall, ribbed floor and three lifters; only the shell is a straight cylinder")


# ---------------------------------------------------------- vending machine


def build_vending_machine(p: PropBuilder) -> None:
    """Vending cabinet: product window, button column and flap, all painted."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = p.set_texture(128, seed=61)
    tex.auto("front", "panel", "side", "trim")

    metal = palette.mix(palette.hex_to_rgb(palette.INSTITUTIONAL_BLUE), palette.hex_to_rgb(palette.METAL_DARK), 0.45)
    metal_light = palette.shade(metal, 1.18)
    metal_dark = palette.shade(metal, 0.6)
    dark = palette.hex_to_rgb(palette.ELECTRONICS_DARK)
    screen = palette.hex_to_rgb(palette.SCREEN_DARK)
    card = palette.shade(metal_light, 1.05)
    products = [
        palette.hex_to_rgb(palette.FABRIC_RED),
        palette.hex_to_rgb(palette.INSTITUTIONAL_GREEN),
        palette.hex_to_rgb(palette.CARDBOARD),
        palette.hex_to_rgb(palette.FABRIC_BLUE),
        palette.hex_to_rgb(palette.BOTTLE_BLUE),
        palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL),
        palette.hex_to_rgb(palette.PLASTIC_CREAM),
    ]

    # --- texture: front elevation ------------------------------------------
    _paint(tex, "front", palette.shade(metal, 0.98), seed=1, grain_density=0.2)
    # Header sign board (top 14% of the front) with an invented wordmark.
    tex.bar("front", palette.shade(metal_dark, 0.92), (0.03, 0.02, 0.97, 0.13))
    tex.bar("front", palette.shade(metal_light, 1.02), (0.06, 0.035, 0.94, 0.115))
    tex.scribble("front", palette.shade(metal_dark, 0.8), (0.12, 0.05, 0.66, 0.105), seed=2, text_blocks=2, alpha=150)
    tex.bar("front", palette.hex_to_rgb(palette.FABRIC_RED), (0.70, 0.045, 0.90, 0.105))
    # Product window: four shelves of painted, entirely fictional packs.
    win = (0.08, 0.184, 0.66, 0.71)
    interior = palette.mix(screen, palette.hex_to_rgb(palette.GLASS_TINT), 0.22)
    tex.bar("front", interior, win)
    rows = 4
    for row in range(rows):
        top = win[1] + (win[3] - win[1]) * (row / rows) + 0.035
        bottom = win[1] + (win[3] - win[1]) * ((row + 1) / rows) - 0.014
        tex.bar("front", palette.shade(metal_light, 1.06), (win[0], bottom, win[2], bottom + 0.014))
        cursor = win[0] + 0.012
        while cursor < win[2] - 0.04:
            product_width = tex.rng.randint(2, 4) / 64.0
            colour = products[tex.rng.randint(0, len(products) - 1)]
            tex.bar("front", palette.shade(colour, 1.12), (cursor, top + tex.rng.randint(0, 2) / 64.0,
                                                            cursor + product_width, bottom))
            tex.bar("front", palette.shade(colour, 0.72), (cursor, bottom - 0.022, cursor + product_width, bottom))
            cursor += product_width + tex.rng.randint(1, 2) / 64.0
    tex.streaks("front", palette.shade(palette.hex_to_rgb(palette.GLASS_TINT), 1.15), count=3, seed=8,
                alpha=26, sub=win)
    tex.band("front", palette.shade(metal_light, 1.1), 0.20, 0.26, alpha=26)
    # Lower front: delivery flap and a vent grille.
    tex.bar("front", palette.shade(metal_dark, 0.9), (0.14, 0.80, 0.60, 0.90))
    for index in range(5):
        tex.bar("front", palette.shade(dark, 1.1), (0.17, 0.825 + index * 0.014, 0.57, 0.833 + index * 0.014), alpha=110)
    _wear(tex, "front", seed=3, rust=4)

    # --- texture: selection column -----------------------------------------
    _paint(tex, "panel", palette.shade(metal_light, 0.98), seed=4, grain_density=0.18)
    tex.border("panel", metal_dark, width=1, alpha=110)
    tex.bar("panel", screen, (0.18, 0.05, 0.82, 0.15))
    tex.scribble("panel", palette.shade(palette.hex_to_rgb(palette.INSTITUTIONAL_GREEN), 1.4),
                 (0.24, 0.075, 0.76, 0.13), seed=5, text_blocks=1, alpha=170)
    for row in range(5):
        for column in range(2):
            fx, fy = 0.32 + column * 0.36, 0.26 + row * 0.12
            tex.dots("panel", palette.shade(metal_light, 1.08), [(fx, fy)], radius=4)
            tex.dots("panel", dark, [(fx, fy)], radius=2)
    tex.bar("panel", dark, (0.30, 0.86, 0.70, 0.90))
    _wear(tex, "panel", seed=6, rust=1)

    # --- texture: flanks and trim -------------------------------------------
    _paint(tex, "side", palette.shade(metal, 0.92), seed=7)
    tex.border("side", palette.shade(metal_dark, 0.9), width=1, alpha=70)
    tex.streaks("side", palette.shade(metal_dark, 0.95), count=2, seed=8, alpha=40)
    _wear(tex, "side", seed=9, rust=4)
    _paint(tex, "trim", palette.shade(metal, 0.84), seed=10, grain_density=0.22)
    tex.border("trim", palette.shade(metal_dark, 0.8), width=1, alpha=80)

    # --- geometry -----------------------------------------------------------
    frame = 0.04
    body_front = half_d - frame
    body = depth - frame
    middle = (body_front - half_d) * 0.5

    p.box(
        (0.0, 0.045, middle),
        (width - 0.08, 0.09, body - 0.08),
        uv=tex.uv("trim"),
        color=palette.shade(metal_dark, 0.8),
        proxy=False,
    )
    p.box(
        (0.0, height * 0.5, middle),
        (width, height, body),
        uv={"+z": tex.uv("front"), "-z": tex.uv("side"), "+y": tex.uv("trim"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=metal,
    )
    # Window surround: two posts and two rails proud of the front, so the
    # painted shelves sit visibly recessed behind glass-height metalwork.
    p.box((-0.46, 1.05, body_front + frame * 0.5), (0.08, 1.16, frame),
          uv=tex.uv("trim"), color=metal_light)
    p.box((-0.13, 1.59, body_front + frame * 0.5), (0.58, 0.08, frame),
          uv=tex.uv("trim"), color=metal_light)
    p.box((-0.13, 0.51, body_front + frame * 0.5), (0.58, 0.08, frame),
          uv=tex.uv("trim"), color=metal_light)
    # Selection column: shallower plate, then the coin bezel and card reader.
    p.box((0.33, 1.05, body_front + 0.01), (0.34, 1.16, 0.02),
          uv={"+z": tex.uv("panel"), "-z": None, "+y": tex.uv("trim"), "-y": None,
              "+x": tex.uv("trim"), "-x": tex.uv("trim")},
          color=metal_light)
    p.box((0.245, 1.60, body_front + 0.03), (0.07, 0.12, 0.02),
          uv=tex.uv("trim"), color=palette.shade(metal_light, 1.05))
    p.box((0.245, 0.60, body_front + 0.03), (0.06, 0.10, 0.02),
          uv=tex.uv("trim"), color=card)
    # Delivery flap.
    p.box((-0.04, 0.26, body_front + 0.01), (0.52, 0.20, 0.02),
          uv=tex.uv("trim"), color=palette.shade(metal, 0.9))
    p.add_note("window products, buttons and slot are paint only; rails and column are the geometry read")


# ------------------------------------------------------------- water cooler


def build_water_cooler(p: PropBuilder) -> None:
    """Cooler cabinet with two taps and an inverted 8-segment bottle."""
    width, height, depth = p.size
    half_d = depth * 0.5

    tex = p.set_texture(64, seed=67)
    tex.auto("front", "side", "bottle", "cap", "top", cols=2, rows=3)

    # Deliberately low-chroma: the cooler is a faded office appliance, and a
    # bottle painted at the palette's own BOTTLE_BLUE out-saturates the whole
    # pack (see the fridge and washing machine for the target grey level).
    metal = palette.shade(palette.mix(palette.hex_to_rgb(palette.METAL_GREY),
                                      palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL), 0.15), 1.02)
    metal_light = palette.shade(metal, 1.08)
    metal_dark = palette.shade(metal, 0.62)
    bottle_grey = palette.mix(
        palette.mix(palette.hex_to_rgb(palette.BOTTLE_BLUE), palette.hex_to_rgb(palette.METAL_GREY), 0.62),
        palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL), 0.16,
    )
    bottle_light = palette.mix(bottle_grey, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.20)
    bottle_deep = palette.mix(bottle_grey, palette.hex_to_rgb(palette.INSTITUTIONAL_TEAL), 0.35)
    # The geometry's vertex colour, lifted so the painted gradient survives the
    # texture * vertex-colour multiply instead of turning the bottle to mud.
    bottle_tint = palette.mix(bottle_light, palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.30)
    plinth_color = palette.shade(metal_dark, 0.7)

    # --- texture: cabinet front --------------------------------------------
    _paint(tex, "front", palette.shade(metal, 0.98), seed=1, grain_density=0.18)
    tex.border("front", metal_dark, width=1, alpha=90)
    tex.bar("front", palette.shade(metal_dark, 0.92), (0.16, 0.22, 0.84, 0.62))
    tex.bar("front", palette.shade(metal_dark, 0.78), (0.22, 0.28, 0.78, 0.56))
    tex.bar("front", palette.shade(metal_dark, 0.95), (0.28, 0.66, 0.72, 0.78))
    _wear(tex, "front", seed=2, rust=2, streaks=2)

    # --- texture: flanks and collar ----------------------------------------
    _paint(tex, "side", palette.shade(metal, 0.93), seed=3)
    tex.border("side", metal_dark, width=1, alpha=70)
    _wear(tex, "side", seed=4, rust=2, streaks=2)
    _paint(tex, "top", metal_light, seed=5, grain_density=0.3)
    tex.border("top", metal_dark, width=1, alpha=80)

    # --- texture: bottle ----------------------------------------------------
    # "Translucent" is a painted gradient plus a soft highlight: no alpha.  The
    # gradient spans only ~20 % of value and the bands are faint, so the bottle
    # does not stripe into bright and dark plastic facets.
    tex.gradient("bottle", bottle_light, bottle_deep, jitter=3, seed=6)
    tex.bar("bottle", palette.shade(bottle_light, 1.05), (0.32, 0.0, 0.42, 1.0), alpha=70)
    tex.band("bottle", palette.shade(bottle_deep, 0.94), 0.42, 0.47, alpha=35)
    tex.band("bottle", palette.shade(bottle_deep, 0.94), 0.70, 0.75, alpha=28)
    # Silt and scuffing: the bottle end (region bottom) sits at the collar.
    tex.band("bottle", palette.hex_to_rgb(palette.GRIME), 0.93, 1.0, alpha=44)
    tex.band("bottle", palette.hex_to_rgb(palette.GRIME), 0.55, 0.60, alpha=26)
    tex.spots("bottle", palette.hex_to_rgb(palette.GRIME), count=6, seed=7, radius=2, alpha=32)
    tex.spots("bottle", palette.shade(bottle_deep, 0.78), count=4, seed=9, radius=2, alpha=28)
    # The bottle's base disc is a large, flat, well-lit face: grubby it up so it
    # does not read as a clean white plastic lid from above.
    tex.gradient("cap", palette.shade(bottle_deep, 0.98), palette.shade(bottle_light, 0.86), jitter=5, seed=8)
    tex.spots("cap", palette.hex_to_rgb(palette.GRIME), count=5, seed=10, radius=2, alpha=34)
    tex.border("cap", palette.shade(bottle_deep, 0.8), width=1, alpha=100)
    tex.dots("cap", palette.shade(bottle_deep, 0.9), [(0.5, 0.5)], radius=8)

    # --- geometry -----------------------------------------------------------
    # A real cooler is roughly half cabinet and half bottle; the 1.1 m
    # catalogue height is spent accordingly -- the cabinet stops at 0.66 and the
    # bottle gets the top 0.40 m, because the bottle is what makes this prop
    # read as a water cooler rather than a small cupboard.
    taps = 0.02
    cabinet_front = half_d - taps
    cabinet_depth = depth - taps
    plinth_h = 0.04
    cabinet_top = 0.66
    collar_top = 0.70   # the bottle neck sits in the cooler's collar

    p.box(
        (0.0, plinth_h * 0.5, -0.015),
        (width - 0.05, plinth_h, cabinet_depth - 0.03),
        uv=tex.uv("side"),
        color=plinth_color,
        proxy=False,
    )
    p.box(
        (0.0, plinth_h + (cabinet_top - plinth_h) * 0.5, -0.01),
        (width, cabinet_top - plinth_h, cabinet_depth),
        uv={"+z": tex.uv("front"), "-z": tex.uv("side"), "+y": tex.uv("top"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=metal,
    )
    p.box(
        (0.0, (cabinet_top + collar_top) * 0.5, -0.01),
        (width - 0.02, collar_top - cabinet_top, cabinet_depth - 0.02),
        uv={"+z": tex.uv("side"), "-z": tex.uv("side"), "+y": tex.uv("top"),
            "-y": None, "+x": tex.uv("side"), "-x": tex.uv("side")},
        color=metal_light,
    )
    # Inverted bottle: a short neck, a quick shoulder and then a nearly
    # straight body -- the silhouette a 19 litre water bottle actually has.
    # Hand-built rather than three toolkit cylinders so the facet shading can
    # be compressed to 0.90..1.00: the stock 0.72..1.00 range stripes a pale
    # bottle into bright and dark plastic bands, which is exactly the showroom
    # look this prop must not have.
    bottle_segments = 8
    # (height, radius): neck mouth, collar, shoulder flare, shoulder, body,
    # body's widest ring and the base's chamfer.
    bottle_rings = (
        (collar_top, 0.050),
        (0.78, 0.057),
        (0.86, 0.118),
        (0.94, 0.146),
        (1.03, 0.152),
        (height - 0.015, 0.143),
        (height, 0.128),
    )
    u0, v0, u1, v1 = tex.uv("bottle")
    cap_u0, cap_v0, cap_u1, cap_v1 = tex.uv("cap")
    cap_centre = ((cap_u0 + cap_u1) * 0.5, (cap_v0 + cap_v1) * 0.5)
    span = bottle_rings[-1][0] - bottle_rings[0][0]
    # Each ring maps to its own slice of the region, so the painted gradient
    # runs once up the bottle instead of repeating on every strip.
    ring_v = [v1 + (v0 - v1) * ((y - bottle_rings[0][0]) / span) for y, _ in bottle_rings]
    for index in range(bottle_segments):
        a0 = math.tau * index / bottle_segments
        a1 = math.tau * (index + 1) / bottle_segments
        t0, t1 = index / bottle_segments, (index + 1) / bottle_segments
        multiplier = 0.90 + 0.10 * (0.5 + 0.5 * math.cos(a0 - 0.9))
        for step in range(len(bottle_rings) - 1):
            lower_y, lower_r = bottle_rings[step]
            upper_y, upper_r = bottle_rings[step + 1]
            lower_v, upper_v = ring_v[step], ring_v[step + 1]
            p.mesh.quad(
                (math.cos(a0) * lower_r, lower_y, math.sin(a0) * lower_r),
                (math.cos(a1) * lower_r, lower_y, math.sin(a1) * lower_r),
                (math.cos(a1) * upper_r, upper_y, math.sin(a1) * upper_r),
                (math.cos(a0) * upper_r, upper_y, math.sin(a0) * upper_r),
                uv=[(u0 + (u1 - u0) * t0, lower_v), (u0 + (u1 - u0) * t1, lower_v),
                    (u0 + (u1 - u0) * t1, upper_v), (u0 + (u1 - u0) * t0, upper_v)],
                color=bottle_tint,
                shade_mult=multiplier,
            )
    for index in range(bottle_segments):
        a0 = math.tau * index / bottle_segments
        a1 = math.tau * (index + 1) / bottle_segments
        p.mesh.triangle(
            (0.0, height, 0.0),
            (math.cos(a1) * bottle_rings[-1][1], height, math.sin(a1) * bottle_rings[-1][1]),
            (math.cos(a0) * bottle_rings[-1][1], height, math.sin(a0) * bottle_rings[-1][1]),
            [cap_centre, (cap_u1, cap_v1), (cap_u0, cap_v1)],
            bottle_tint,
            shade_mult=0.97,
        )
    # Two taps and a drip tray on the front face.
    for sx in (-1.0, 1.0):
        p.cylinder((sx * 0.055, 0.50, cabinet_front), 0.019, taps, segments=6, axis="z",
                   side_uv=tex.uv("side"), cap_uv=tex.uv("side"), color=palette.hex_to_rgb(palette.CHROME))
    p.box((0.0, 0.42, cabinet_front + taps * 0.5), (0.15, 0.014, taps),
          uv=tex.uv("side"), color=palette.hex_to_rgb(palette.CHROME), proxy=False)
    p.add_note("bottle translucency is a painted gradient on an 8-segment taper; taps are geometry")


PROPS = {
    "core:stove": build_stove,
    "core:sink": build_sink,
    "core:fridge": build_fridge,
    "core:washing_machine": build_washing_machine,
    "core:vending_machine": build_vending_machine,
    "core:water_cooler": build_water_cooler,
    "core:washer_drum": build_washer_drum,
}

"""The Home theme's props: kitchen cabinetry and a small domestic set.

The pack contract (see ``tools/props/README.md`` and the commented exemplar in
``parts/utility.py``) applies unchanged: metres, origin on the floor-contact
point, ``+Z`` facing the player, one 128x128 texture per prop, colours from
:mod:`palette` and the shared construction helpers from
:mod:`parts.furniture`.

The two cabinets are the clean end of the pack -- an ordinary off-white shaker
kitchen rather than the faded institutional set.  There is deliberately no
rust, grime or wear: the only shading is a painted shaker recess, a whisper of
brush grain and the dark underside of the wall unit.  Panel seams are painted
on the texture, so each door leaf is a plain box; the carcass, the counter
slab, the recessed toe kick and the small bar handles carry the silhouette.

The domestic set extends the same tidy finish:

* ``home:ball_light`` -- a glowing opal orb on a cord, the pack's first
  emissive prop along with the exit sign;
* ``home:wall_switch`` -- a plate and a hinged rocker carrying the rigid
  ``toggle`` clip (no skins: a pivot node and a child mesh);
* ``home:crt_tv`` -- a beige CRT with a curved, switched-off screen.
"""

from __future__ import annotations

import math
from pathlib import Path

import palette
from mesh import PropBuilder
from parts.furniture import _solid, _tint
from parts.refreshed import solid_cylinder, solid_box, padded_box, orient_outward, load_atlas_from

# Triangle aims (the pack budget in ``build.py`` is the enforced one).
TARGET = {
    "home:cabinet_base": 90,
    "home:cabinet_wall": 70,
    "home:ball_light": 190,
    "home:wall_switch": 70,
    "home:crt_tv": 320,
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


# --------------------------------------------------------------- domestic set


def build_ball_light(p: PropBuilder) -> None:
    """Hanging ball light: a glowing opal orb on a straight cord and rose.

    The orb is its own emissive material group (``emissiveFactor`` plus
    ``KHR_materials_emissive_strength``), so the painted gloss and the white
    hot centre read as a lit globe in a dark room; the level owns the real
    light.  The fixture is rotationally symmetric, so ``+Z`` only orients the
    painted highlight.  The base is the orb's underside (y = 0), and the
    catalogue height includes the ceiling rose, so a level authors
    ``y = ceiling_y - 0.80``.
    """
    source = Path(__file__).resolve().parents[3] / "assets/environment/home/props/models/ball_light.png"
    tex = load_atlas_from(p, source, ('orb', 'cord', 'rose'))

    orb_uv = tex.uv("orb")
    cord_uv = tex.uv("cord")
    rose_uv = tex.uv("rose")

    orb_slot = p.material("orb_glow", emissive=(1.0, 0.97, 0.92), strength=1.2)
    cord_slot = p.material("cord_body")

    # A sphere lathe with small pole rings closed by caps: a zero-radius pole
    # ring would make the first and last bands degenerate quads.
    orb_radius = 0.10
    pole_radius = 0.014
    pole_y = math.sqrt(orb_radius * orb_radius - pole_radius * pole_radius)
    profile = [(-pole_y, pole_radius)]
    profile += [
        (orb_radius * math.sin(math.radians(angle)), orb_radius * math.cos(math.radians(angle)))
        for angle in (-60.0, -30.0, 0.0, 30.0, 60.0)
    ]
    profile += [(pole_y, pole_radius)]
    p.begin_material(orb_slot)
    orb_start = len(p.mesh.indices)
    p.lathe((0.0, orb_radius, 0.0), profile, segments=12, axis="y", uv=orb_uv, color=(255, 255, 255))

    orient_outward(p.mesh, orb_start, (0.0, orb_radius, 0.0))
    p.begin_material(cord_slot)
    solid_cylinder(p, (0.0, 0.193, 0.0), 0.019, 0.026, segments=8, uv=rose_uv, color=(240, 240, 238))
    p.cylinder((0.0, 0.20, 0.0), 0.004, 0.58, segments=6, uv=cord_uv,
               color=_tint(palette.hex_to_rgb(palette.PLASTIC_GREY), 0.35), bottom=False)
    solid_cylinder(p, (0.0, 0.78, 0.0), 0.05, 0.02, segments=10, uv=rose_uv,
                   color=_tint(palette.hex_to_rgb(palette.PLASTIC_WHITE), 0.45))

    p.add_note("orb is its own emissive material (strength 1.2); level authors a point light 2 cm below the orb")
    p.add_note("straight cord and ceiling rose; base is the orb underside")


SWITCH_TILT_DEG = 15.0


def _quaternion_x(degrees: float) -> list[float]:
    """glTF quaternion (x, y, z, w) for a rotation about +X."""
    half = math.radians(degrees) * 0.5
    return [math.sin(half), 0.0, 0.0, math.cos(half)]


def build_wall_switch(p: PropBuilder) -> None:
    """Wall light switch: an off-white plate with a hinged rocker.

    The rocker is a second glTF mesh on a pivot node and the model carries one
    LINEAR clip named ``toggle``.  The clip's first key is the bind pose
    (identity on the pivot, the up-tilt authored into the rocker geometry) and
    its last key tilts the rocker 30 degrees down.  There is deliberately no
    ``skins`` array: this is a rigid node animation, and the rocker node's
    counter-translation keeps the composed rest transform the identity, so the
    authored model-space vertices and the measured bounds are exact.

    Placement: the plate's back plane is ``z = 0`` and ``+Z`` leaves the wall.
    Put the instance's ``z`` on the wall surface, ``y`` about 1.15 and rotate
    so ``+Z`` faces the room (``rotation_degrees`` 0/90/180/270).
    """
    source = Path(__file__).resolve().parents[3] / "assets/environment/home/props/models/wall_switch.png"
    tex = load_atlas_from(p, source, ('plate', 'rocker', 'metal'))

    plate_uv = tex.uv("plate")
    rocker_uv = tex.uv("rocker")
    metal_uv = tex.uv("metal")
    plate_tint = _tint(PAINT, 0.85)
    rocker_tint = _tint(PAINT, 0.80)
    metal_tint = _tint(palette.hex_to_rgb(palette.METAL_GREY), 0.35)

    slot = p.material("switch_body")
    p.begin_material(slot)

    p.begin_mesh("plate")
    padded_box(p, (0.0, 0.06, 0.006), (0.086, 0.12, 0.012), plate_uv, bevel=0.003)
    for screw_y in (0.014, 0.106):
        solid_cylinder(p, (0.0, screw_y, 0.012), 0.004, 0.002, segments=8,
                       axis="z", uv=metal_uv, color=metal_tint)
    # Dark recess frames the moving rocker; the pivot and clip stay unchanged.
    solid_box(p, (0.0, 0.075, 0.012), (0.036, 0.048, 0.003),
              uv=metal_uv, color=(110, 112, 108))
    p.begin_mesh("rocker")
    padded_box(p, (0.0, 0.075, 0.014), (0.030, 0.042, 0.010), rocker_uv,
               bevel=0.0018, rotation=(SWITCH_TILT_DEG, 0.0, 0.0))

    # The pivot carries the hinge translation; the rocker node cancels it so
    # the composed rest transform is the identity and the authored vertices sit
    # exactly where they were drawn.
    p.node("switch", mesh="plate", children=[1])
    p.node("lever_pivot", translation=(0.0, 0.075, 0.014), children=[2])
    p.node("lever", translation=(0.0, -0.075, -0.014), mesh="rocker")
    p.clip("toggle", [
        {
            "node": 1,
            "path": "rotation",
            "times": [0.0, 0.35],
            "values": [[0.0, 0.0, 0.0, 1.0], _quaternion_x(-30.0)],
            "interpolation": "LINEAR",
        }
    ])

    p.add_note("rigid node animation, no skins; clip `toggle`: first key = bind pose, last key -30 deg about X")
    p.add_note("plate back is z = 0, +Z leaves the wall; base (plate bottom) is y = 0")


def build_crt_tv(p: PropBuilder) -> None:
    """CRT television: a beige cabinet, a proud bezel, a curved dark screen
    and two knobs.  Deliberately unlit like ``core:tv``: no emissive material
    and no prop light, so it sits in the pack's switched-off electronics set.
    """
    source = Path(__file__).resolve().parents[3] / "assets/environment/home/props/models/crt_tv.png"
    tex = load_atlas_from(p, source, ('screen', 'bezel', 'body', 'panel'))
    case = palette.mix(palette.hex_to_rgb(palette.PLASTIC_BEIGE), palette.hex_to_rgb(palette.WOOD_MID), 0.30)
    case_dark = palette.shade(case, 0.68)
    screen = palette.hex_to_rgb(palette.SCREEN_DARK)

    screen_uv = tex.sub("screen", 0.0, 0.0, 1.0, 0.75)
    body_uv = tex.uv("body")
    bezel_uv = tex.uv("bezel")
    panel_uv = tex.uv("panel")
    body_tint = _tint(case, 0.55)
    bezel_tint = _tint(case, 0.45)
    screen_tint = _tint(screen, 0.45)
    foot_tint = _tint(case_dark, 0.50)

    # Feet: 0.05 m cubes inset from the cabinet corners, top face buried.
    for side_x in (-1.0, 1.0):
        for side_z in (-1.0, 1.0):
            _solid(p, (side_x * 0.225, 0.03, side_z * 0.15), (0.05, 0.06, 0.05),
                   body_uv, foot_tint, hidden=("+y",))
    # A tapered tube housing: wide front, narrower and lower rear bell.
    front = [(-0.275, 0.06, 0.20), (0.275, 0.06, 0.20),
             (0.275, 0.48, 0.20), (-0.275, 0.48, 0.20)]
    rear = [(-0.195, 0.10, -0.24), (0.195, 0.10, -0.24),
            (0.195, 0.42, -0.24), (-0.195, 0.42, -0.24)]
    start = len(p.mesh.indices)
    for i in range(4):
        j = (i + 1) % 4
        p.mesh.quad(front[i], rear[i], rear[j], front[j], uv=body_uv, color=body_tint)
    p.mesh.quad(*rear, uv=body_uv, color=body_tint)
    orient_outward(p.mesh, start, (0.0, 0.27, 0.0))
    # Rear cooling slots remain silhouette-scale details rather than noise.
    for i in range(7):
        solid_box(p, (-0.12 + i * 0.04, 0.31, -0.241), (0.012, 0.11, 0.002),
                  uv=body_uv, color=(92, 91, 85))

    # Bezel bars proud of the cabinet face, framing a 0.38 x 0.28 aperture.
    for side_x in (-1.0, 1.0):
        _solid(p, (side_x * 0.2325, 0.27, 0.209), (0.085, 0.42, 0.018),
               bezel_uv, bezel_tint, hidden=("-z",))
    _solid(p, (0.0, 0.445, 0.209), (0.38, 0.07, 0.018), bezel_uv, bezel_tint, hidden=("-z",))
    # The bottom bar is the control panel: painted speaker slots, a power dot
    # and the knob legends.
    _solid(p, (0.0, 0.095, 0.209), (0.38, 0.07, 0.018),
           {"+z": panel_uv, "+x": bezel_uv, "-x": bezel_uv, "+y": bezel_uv, "-y": bezel_uv},
           bezel_tint, hidden=("-z",))

    # Curved screen: a 5 x 4 lattice bowed 12 mm towards the viewer, its front
    # edges tucked under the bars so no seam shows at an oblique angle.
    columns, rows = 5, 4
    half_screen_x = 0.195
    screen_top, screen_bottom = 0.415, 0.125
    for column in range(columns - 1):
        for row in range(rows - 1):
            u0 = column / (columns - 1)
            u1 = (column + 1) / (columns - 1)
            w0 = screen_bottom + (screen_top - screen_bottom) * row / (rows - 1)
            w1 = screen_bottom + (screen_top - screen_bottom) * (row + 1) / (rows - 1)
            x0, x1 = -half_screen_x + 2.0 * half_screen_x * u0, -half_screen_x + 2.0 * half_screen_x * u1
            z0 = 0.188 + 0.012 * (1.0 - (x0 / half_screen_x) ** 2)
            z1 = 0.188 + 0.012 * (1.0 - (x1 / half_screen_x) ** 2)
            quads = [
                (x0, w0, z0),
                (x1, w0, z1),
                (x1, w1, z1),
                (x0, w1, z0),
            ]
            su0 = screen_uv[0] + (screen_uv[2] - screen_uv[0]) * u0
            su1 = screen_uv[0] + (screen_uv[2] - screen_uv[0]) * u1
            sv0 = screen_uv[3] - (screen_uv[3] - screen_uv[1]) * row / (rows - 1)
            sv1 = screen_uv[3] - (screen_uv[3] - screen_uv[1]) * (row + 1) / (rows - 1)
            p.mesh.quad(*quads, uv=[(su0, sv0), (su1, sv0), (su1, sv1), (su0, sv1)],
                        color=(240, 246, 245))


    # Two knobs on the bottom bar, axis +Z.
    p.cylinder((0.06, 0.095, 0.218), 0.025, 0.012, segments=8, axis="z", uv=bezel_uv,
               color=bezel_tint, bottom=False)
    p.cylinder((0.13, 0.095, 0.218), 0.0175, 0.012, segments=8, axis="z", uv=bezel_uv,
               color=bezel_tint, bottom=False)

    p.add_note("CRT: beige case, proud bezel bars, curved switched-off screen, two knobs")
    p.add_note("unlit like core:tv (no emissive material, no prop light)")


PROPS = {
    "home:cabinet_base": build_cabinet_base,
    "home:cabinet_wall": build_cabinet_wall,
    "home:ball_light": build_ball_light,
    "home:wall_switch": build_wall_switch,
    "home:crt_tv": build_crt_tv,
}

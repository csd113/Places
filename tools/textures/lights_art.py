#!/usr/bin/env python3
"""Light fixture surface artwork: the visible face of every built-in fixture.

A fixture's *mesh* is generated geometry (a panel, a shallow can, a wall
housing), but what that geometry shows is ordinary external PNG artwork exactly
like a decal sheet: the catalog's `asset_type: "light"` entry names the sheet,
the renderer resolves it through the shared catalog -> PNG -> texture-cache
path, and replacing the PNG needs no Rust change and no recompilation.

Each sheet is the fixture's own visible surface, mapped once across the face it
draws (no world tiling, so nothing here is tileable and nothing wraps):

* ``core:fluorescent_panel_01`` -- 256x128, the flat luminous panel face. Image
  ``x`` is the panel's 1.2 m width axis and image ``y`` its 0.6 m depth axis, so
  one texel is 4.7 mm in both directions. The artwork is a twin-tube diffuser:
  two soft tube bands under an off-white acrylic sheet, a restrained falloff
  towards the frame, fine grain and a little dust along the edges. It must read
  as a *surface*, never as baked illumination: the room lighting is the bake's
  job (see ``src/lighting.rs``).
* ``core:pool_light_round`` -- 128x128, the round diffuser seen face-on: image
  centre is the fixture centre, the inscribed circle is the diffuser's own
  radius (0.22 m), and one texel is 3.9 mm. Concentric moulding rings, faint
  radial ribs, a bright lamp core and a dark contact line where the diffuser
  meets its bezel.
* ``core:pool_light_wall`` -- 128x64, the wall luminaire's lens face: 0.4 m
  wide by 0.2 m tall, 3.1 mm per texel. An opal cover with a soft horizontal
  lamp band, vertical ribbing and a slightly shadowed cover edge.

The palette stays pale and near-neutral on purpose: the fixture's vertex colour
is a *neutral* emission strength (the intensity response, never the authored
light colour) multiplied into the sampled texel, so an off fixture darkens this
artwork to black without any code knowing about the texture, while a coloured
light changes only the illumination it bakes into the room.

Everything is deterministic -- only :mod:`artkit` helpers, no randomness, no
clock, no external images.
"""

from __future__ import annotations

import math

from artkit import (
    Canvas,
    fbm,
    hash01,
    smoothstep,
    tile_noise,
)

# ---------------------------------------------------------------- fluorescent
#
# The office panel's 1.2 x 0.6 m face at 256x128: 213 px per metre in both
# directions, so the two tube bands and the edge falloff keep their real
# proportions instead of stretching across the long axis.

PANEL_SIZE_X = 256
PANEL_SIZE_Y = 128

DIFFUSER_BASE = (235.0, 234.5, 230.0)  # off-white acrylic sheet
TUBE_GAIN = 0.052                      # how much brighter a tube reads through it
EDGE_FALLOFF = 0.058                   # how much the panel dims towards its frame
DUST_STRENGTH = 0.030                  # edge dust, well under a visible smudge

# Tube centres as a fraction of the panel's depth axis: a twin-tube fixture
# spreads its lamps across the short axis, one either side of the centre line.
TUBE_CENTRES = (0.32, 0.68)
TUBE_HALF_WIDTH = 26.0                 # px; ~0.12 m of the 0.6 m depth


def _panel_tone(x: int, y: int) -> float:
    """Diffuser tone before the base colour: tubes, falloff, grain and dust."""
    half_x = PANEL_SIZE_X * 0.5
    half_y = PANEL_SIZE_Y * 0.5
    # Elliptical distance from the panel centre, 0 at the centre and 1 on the
    # frame. The falloff only starts past the tubes, so the middle stays broad
    # and flat and the edges settle instead of the whole panel reading as one
    # gradient.
    dx = (x + 0.5 - half_x) / half_x
    dy = (y + 0.5 - half_y) / half_y
    radial = min(1.0, math.sqrt(dx * dx + dy * dy))
    tone = 1.0 - EDGE_FALLOFF * smoothstep(radial, 0.52, 1.0)

    # Twin tubes under the sheet: a smooth bump per lamp, never a bright bar.
    for centre in TUBE_CENTRES:
        distance = abs(y + 0.5 - centre * PANEL_SIZE_Y)
        tone *= 1.0 + TUBE_GAIN * (1.0 - smoothstep(distance, 0.0, TUBE_HALF_WIDTH))

    # Brightness irregularity: fine grain, a low-frequency mottle, and a long
    # 1-2 m drift. Small amplitudes only; the panel is a diffuser, not a page.
    # The total stays under the peak the palette allows, so no texel clips to
    # flat white and the tube bands keep their shape.
    tone *= 1.0 + (hash01(x, y, 401) - 0.5) * 0.013
    tone *= 1.0 + (tile_noise(x, y, PANEL_SIZE_X, 6, 409) - 0.5) * 0.017
    tone *= 1.0 + (fbm(x, y, PANEL_SIZE_X, 3, 9, 21, 419) - 0.5) * 0.022

    # A little dust settles along the frame and into the diffuser's own joints.
    edge_distance = min(x, PANEL_SIZE_X - 1 - x, y, PANEL_SIZE_Y - 1 - y)
    dust = 1.0 - smoothstep(float(edge_distance), 0.0, 6.0)
    smudge = smoothstep(tile_noise(x, 0, PANEL_SIZE_X, 24, 421), 0.80, 0.98)
    tone *= 1.0 - DUST_STRENGTH * (0.65 * dust + 0.35 * smudge)
    return tone


def build_fluorescent_panel() -> Canvas:
    """The office fluorescent panel face: a twin-tube acrylic diffuser."""
    canvas = Canvas(PANEL_SIZE_X, PANEL_SIZE_Y)
    for y in range(PANEL_SIZE_Y):
        for x in range(PANEL_SIZE_X):
            tone = _panel_tone(x, y)
            # The tubes sit marginally cooler than the aged sheet around them.
            warm = (tone - 1.0) * 0.35
            canvas.set(
                x,
                y,
                (
                    DIFFUSER_BASE[0] * tone * (1.0 - 0.012 * warm),
                    DIFFUSER_BASE[1] * tone,
                    DIFFUSER_BASE[2] * tone * (1.0 + 0.016 * warm),
                ),
            )
    return canvas


# ---------------------------------------------------------------- pool, round
#
# The round downlight's diffuser seen face-on. The geometry maps the fixture
# plane onto the sheet, so image centre = fixture centre and the inscribed
# circle = the diffuser radius (0.22 m): 128 px across 0.5 m, 3.9 mm per texel.

ROUND_SIZE = 128
ROUND_CENTRE = (ROUND_SIZE - 1) * 0.5   # 63.5, so the disc is symmetric
ROUND_INSCRIBED = ROUND_SIZE * 0.5      # px radius of the diffuser

ROUND_LAMP_EDGE = 0.12                  # image radius fraction where the diffuser starts
ROUND_RIM_START = 0.88                  # image radius fraction where its edge begins
ROUND_RIM = (146.0, 147.0, 145.0)       # the diffuser's shadowed edge
ROUND_LAMP = (250.0, 249.0, 244.0)      # the lamp core behind the diffuser
ROUND_DIFFUSER = (233.0, 232.5, 228.5)  # the opal diffuser itself
ROUND_RING_TONE = 0.020                 # contrast of one moulded ring
ROUND_RIB_TONE = 0.014                  # contrast of one radial prism rib
ROUND_RIB_COUNT = 32                    # radial prisms; divides the sheet evenly


def _round_rib(x: int, y: int) -> float:
    """Faint radial prism ribs: 32 around the disc, seamless in the image."""
    dx = x + 0.5 - ROUND_CENTRE
    dy = y + 0.5 - ROUND_CENTRE
    if dx == 0.0 and dy == 0.0:
        return 0.0
    angle = (math.atan2(dy, dx) + math.pi) / math.tau
    position = angle * ROUND_RIB_COUNT
    return (position - math.floor(position)) - 0.5


def build_pool_light_round() -> Canvas:
    """The pool downlight's diffuser face: opal disc, lamp core, shadowed edge."""
    canvas = Canvas(ROUND_SIZE, ROUND_SIZE)
    for y in range(ROUND_SIZE):
        for x in range(ROUND_SIZE):
            dx = x + 0.5 - ROUND_CENTRE
            dy = y + 0.5 - ROUND_CENTRE
            radius = math.sqrt(dx * dx + dy * dy) / ROUND_INSCRIBED
            if radius <= ROUND_LAMP_EDGE:
                # The lamp recess behind the diffuser's centre hole: unseen
                # geometry, but it is what the inner edge blends into, so it
                # stays bright rather than black.
                ease = smoothstep(radius, 0.0, ROUND_LAMP_EDGE)
                r = ROUND_LAMP[0] * (1.0 - ease) + ROUND_DIFFUSER[0] * ease
                g = ROUND_LAMP[1] * (1.0 - ease) + ROUND_DIFFUSER[1] * ease
                b = ROUND_LAMP[2] * (1.0 - ease) + ROUND_DIFFUSER[2] * ease
            elif radius < ROUND_RIM_START:
                # Diffuser: brightest next to the lamp, dimming gently outwards.
                tone = 1.0 - 0.135 * smoothstep(radius, ROUND_LAMP_EDGE, 0.96)
                # Three concentric moulding rings with a lit lip inside each.
                for ring in (0.34, 0.55, 0.76):
                    band = 1.0 - smoothstep(abs(radius - ring), 0.0, 0.022)
                    tone *= 1.0 - ROUND_RING_TONE * band
                    lip = 1.0 - smoothstep(abs(radius - (ring - 0.030)), 0.0, 0.014)
                    tone *= 1.0 + ROUND_RING_TONE * 0.75 * lip
                tone *= 1.0 + ROUND_RIB_TONE * _round_rib(x, y)
                tone *= 1.0 + (hash01(x, y, 601) - 0.5) * 0.013
                tone *= 1.0 + (tile_noise(x, y, ROUND_SIZE, 5, 607) - 0.5) * 0.022
                r = ROUND_DIFFUSER[0] * tone
                g = ROUND_DIFFUSER[1] * tone
                b = ROUND_DIFFUSER[2] * tone
                # A thin contact shadow where the diffuser meets its bezel.
                joint = smoothstep(radius, ROUND_RIM_START - 0.055, ROUND_RIM_START)
                r = r * (1.0 - joint) + ROUND_RIM[0] * 0.82 * joint
                g = g * (1.0 - joint) + ROUND_RIM[1] * 0.82 * joint
                b = b * (1.0 - joint) + ROUND_RIM[2] * 0.82 * joint
            else:
                # The diffuser's outer edge, and the sheet's corners: the metal
                # the diffuser sits in. Never sampled as a surface, but it is
                # what the edge blends into and what a distant mip averages
                # against, so it stays a plausible dark grey rather than black.
                outside = smoothstep(radius, ROUND_RIM_START, 1.0)
                tone = 0.92 + 0.08 * (1.0 - outside)
                tone *= 1.0 + (hash01(x, y, 617) - 0.5) * 0.010
                r, g, b = (channel * tone for channel in ROUND_RIM)
            canvas.set(x, y, (r, g, b))
    return canvas


# ------------------------------------------------------------------ wall light
#
# The wall luminaire's lens face: 0.4 m wide by 0.2 m tall at 128x64, so one
# texel is 3.1 mm. The housing around it is generated geometry with a flat
# metal colour; only the lens face is artwork.

WALL_SIZE_X = 128
WALL_SIZE_Y = 64

WALL_LENS = (236.0, 235.5, 232.0)      # opal polycarbonate cover
WALL_COVER_EDGE = 0.86                 # the cover's own shaded lip
WALL_BAND_GAIN = 0.048                 # the horizontal lamp behind the cover
WALL_RIB_COUNT = 16                    # vertical ribs, 8 px apart


def _wall_tone(x: int, y: int) -> float:
    """Lens tone: lamp band, ribbing, edge falloff, grain and a little dust."""
    half_x = WALL_SIZE_X * 0.5
    half_y = WALL_SIZE_Y * 0.5
    # The lamp runs horizontally across the face, so brightness falls away
    # towards the top and bottom cover edges rather than to a point.
    vertical = abs(y + 0.5 - half_y) / half_y
    tone = 1.0 + WALL_BAND_GAIN * (1.0 - smoothstep(vertical, 0.0, 1.0))
    tone *= 1.0 - 0.052 * smoothstep(vertical, 0.62, 1.0)

    # Fine vertical ribs in the moulded cover.
    rib = (x % WALL_RIB_COUNT) / float(WALL_RIB_COUNT) - 0.5
    tone *= 1.0 + 0.013 * rib

    # The cover's border: a narrow shaded lip all the way round.
    edge = min(x, WALL_SIZE_X - 1 - x, y, WALL_SIZE_Y - 1 - y)
    tone *= 1.0 - 0.075 * (1.0 - smoothstep(float(edge), 0.0, 2.5))

    tone *= 1.0 + (hash01(x, y, 701) - 0.5) * 0.014
    tone *= 1.0 + (tile_noise(x, y, WALL_SIZE_X, 6, 709) - 0.5) * 0.018
    # A whisper of dust towards the outer corners of the cover.
    outer = math.sqrt(((x + 0.5 - half_x) / half_x) ** 2 + ((y + 0.5 - half_y) / half_y) ** 2)
    tone *= 1.0 - 0.022 * smoothstep(outer, 0.78, 1.05)
    return tone


def build_pool_light_wall() -> Canvas:
    """The pool wall luminaire's lens face: a ribbed opal cover."""
    canvas = Canvas(WALL_SIZE_X, WALL_SIZE_Y)
    for y in range(WALL_SIZE_Y):
        for x in range(WALL_SIZE_X):
            tone = _wall_tone(x, y)
            r = WALL_LENS[0] * tone
            g = WALL_LENS[1] * tone
            b = WALL_LENS[2] * tone
            # The cover's lip is the only part that reads as a hard edge.
            edge = min(x, WALL_SIZE_X - 1 - x, y, WALL_SIZE_Y - 1 - y)
            if edge == 0:
                lip = WALL_COVER_EDGE
                r, g, b = (channel * lip for channel in (r, g, b))
            canvas.set(x, y, (r, g, b))
    return canvas


# ------------------------------------------------------------------ manifest

ART = {
    "core:fluorescent_panel_01": {
        "model": "environment/office/textures/lights/fluorescent_panel_01.png",
        "build": build_fluorescent_panel,
        "kind": "light",
    },
    "core:pool_light_round": {
        "model": "environment/pool/textures/lights/pool_light_round_01.png",
        "build": build_pool_light_round,
        "kind": "light",
    },
    "core:pool_light_wall": {
        "model": "environment/pool/textures/lights/pool_light_wall_01.png",
        "build": build_pool_light_wall,
        "kind": "light",
    },
}

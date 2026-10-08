#!/usr/bin/env python3
"""Outdoor night artwork: ground and house surfaces, the star sky, feather decals.

The outdoor set is the first environment painted for the game's night look:
a stylized low-poly PS2 / early-Source palette, muted and low-contrast, with
small-scale detail that only has to read under a small pool of warm lamplight.
Nothing here is photographic, nothing bakes sunlight in, and no sheet carries a
bright or dark band along its edges -- a repeated surface must look the same
under the lamp as it does anywhere else.

Painting rules for the five surface sheets:

* 512x512, 8-bit RGBA, opaque (alpha 255), tileable in both axes.  Every
  structured feature (siding boards, shingle courses, tab joints, gravel
  blocks) is phase-offset so a joint never lands on a wrapped sheet edge, and
  the noise helpers wrap at the sheet, so ``tools/textures/seam_repair.py
  --check`` passes on both axes and every channel;
* low-contrast night albedo: the per-texel grain stays inside roughly a tenth
  of the base tone and the structural edges (board and course shadows) stay
  soft, so the lamp pool -- not the artwork -- does the lighting;
* deliberately plain and deterministic: only :mod:`artkit` helpers plus a
  tiny local LCG, no randomness from the clock, no external images.

The sky sheet is separate from the tileable surfaces: it is the equirectangular
environment map, near-black (never pure black), with a small number of faint
star pinpricks in the upper half only, no moon, no Milky Way, no glow, no
gradient and no horizon line.  Stars are inset from all four edges, so the
wrapped u edges join through untouched background.

The three path decals are *fitted* sheets with real alpha gradients (no
dithering), painted by the same module but never seam-gated -- their fade
directions are documented on each painter:

* :func:`build_path_edge` -- full alpha along -U (the path side), fading to 0
  at the +U sheet edge, with a short taper at both v ends;
* :func:`build_path_end` -- a radial fade, alpha 0 on every edge pixel;
* :func:`build_path_corner` -- an L-shaped path (bottom and right edges) whose
  two inner fades run out along -U and -V and meet at a rounded inner corner
  near the sheet centre.

``ART`` maps each logical texture id to its catalog ``model`` path and painter,
which ``build.py`` merges into the manifest.  The PNGs are the authoritative
runtime assets: the game loads them at level load and never runs this script.
"""

from __future__ import annotations

import math

from artkit import (
    Canvas,
    fbm,
    hash01,
    lattice_hash01,
    mix_rgb,
    scaled,
    smoothstep,
    tile_noise,
    tile_noise2,
)

# Tileable surface sheets are square; 512 is the preferred source resolution
# for this set (the hard ceiling is 1024).
SIZE = 512

# ------------------------------------------------------------------ palette
#
# Albedo before the baked night lighting.  Everything is desaturated and low
# contrast: the outdoor materials author no tint, so these values are what the
# renderer samples.

GRASS_BASE = (62.0, 77.0, 50.0)     # short mown night lawn
GRASS_DARK = (44.0, 57.0, 36.0)     # shade between the tufts
GRASS_LIGHT = (84.0, 100.0, 66.0)   # catching a little of the sky

DIRT_BASE = (93.0, 83.0, 65.0)      # walked packed dirt
DIRT_DARK = (60.0, 53.0, 42.0)      # damp hollows and pebbles
DIRT_LIGHT = (120.0, 110.0, 92.0)   # dry scuffs
GRAVEL_PALE = (138.0, 134.0, 122.0)  # the pale chips scattered in the path

CONCRETE_BASE = (136.0, 137.0, 132.0)  # poured walkway, deliberately grey-green
CONCRETE_DARK = (104.0, 106.0, 102.0)  # pores and weathered hollows
CONCRETE_LIGHT = (156.0, 157.0, 152.0)  # fine exposed aggregate

SIDING_BASE = (70.0, 82.0, 72.0)    # painted clapboard, dark green-grey
SIDING_DARK = (44.0, 53.0, 46.0)
SIDING_LIGHT = (92.0, 104.0, 92.0)
SIDING_SHADOW = (32.0, 39.0, 33.0)  # the shadow line under each board overlap

SHINGLE_BASE = (56.0, 54.0, 58.0)   # dark asphalt shingles
SHINGLE_DARK = (36.0, 35.0, 39.0)
SHINGLE_LIGHT = (74.0, 72.0, 76.0)
SHINGLE_SHADOW = (26.0, 26.0, 30.0)  # the shadow each course casts on the next

SKY_BASE = (7, 9, 14)               # near-black, a whisper of blue
SKY_STAR_MIN = 58                   # faintest star brightness on the base
SKY_STAR_SPAN = 95                  # brightest star is SKY_STAR_MIN + span - 1
SKY_STAR_COUNT = 100                # within the requested 60..140 pinpricks
SKY_BRIGHT_CUTOFF = 126             # brighter stars get a second pixel

DECAL_CLEAR = (0, 0, 0, 0)


# ------------------------------------------------------------------- helpers


def _cell_hash(x: int, y: int, cell: int, seed: int) -> float:
    """One deterministic value per ``cell``-sized block, wrapping at ``SIZE``.

    ``cell`` must divide :data:`SIZE`.  The block index is taken modulo the
    blocks per sheet, so a feature field built from ``_cell_hash`` is exactly
    periodic and joins its own opposite edges.
    """
    period = SIZE // cell
    return hash01((x // cell) % period, (y // cell) % period, seed)


def _streak(x: int, y: int, cell_x: int, cell_y: int, seed: int) -> float:
    """Wrapped value noise on an anisotropic lattice: horizontal grain.

    The same smoothstep-interpolated lattice as :func:`artkit.tile_noise`, but
    with independent axis cells, so a long ``cell_x`` and a short ``cell_y``
    stretch every feature into a horizontal streak (painted wood grain on the
    siding).  Both axes wrap at the sheet.
    """
    fx = x / cell_x
    fy = y / cell_y
    x0 = math.floor(fx)
    y0 = math.floor(fy)
    tx = fx - x0
    ty = fy - y0
    sx = tx * tx * (3.0 - 2.0 * tx)
    sy = ty * ty * (3.0 - 2.0 * ty)
    nx = SIZE // cell_x
    ny = SIZE // cell_y
    ix0 = int(x0) % nx
    ix1 = (int(x0) + 1) % nx
    iy0 = int(y0) % ny
    iy1 = (int(y0) + 1) % ny
    v00 = lattice_hash01(ix0, iy0, seed)
    v10 = lattice_hash01(ix1, iy0, seed)
    v01 = lattice_hash01(ix0, iy1, seed)
    v11 = lattice_hash01(ix1, iy1, seed)
    top = v00 + (v10 - v00) * sx
    bottom = v01 + (v11 - v01) * sx
    return top + (bottom - top) * sy


class _Rng:
    """Tiny deterministic LCG, matching the level fixture generators.

    Python's :mod:`random` is seeded per process and version; this fixed
    multiplies-and-adds stream is stable across platforms, so the sky and the
    scatter tool are byte-reproducible.
    """

    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFF_FFFF

    def next(self) -> int:
        self.state = (self.state * 1_664_525 + 1_013_904_223) & 0xFFFF_FFFF
        return self.state

    def unit(self) -> float:
        return self.next() / 0xFFFF_FFFF


# --------------------------------------------------------------- dirt grain
#
# Shared by the dirt surface and all three path decals so a feather blends into
# the path material it extends.  Different call sites pass different seeds so
# the decal never looks like a copy of the tile.


def _dirt_rgb(x: int, y: int, seed: int) -> tuple[float, float, float]:
    """One texel of packed dirt with scattered pale gravel chips."""
    broad = fbm(x, y, SIZE, 2, 6, 17, seed) - 0.5
    clod = tile_noise(x, y, SIZE, 12, seed + 3) - 0.5
    fine = hash01(x, y, seed + 5) - 0.5
    tone = 1.0 + broad * 0.085 + clod * 0.060 + fine * 0.100
    colour = scaled(DIRT_BASE, tone)
    # 2x2-pixel chips on a wrapped block field: ~1.4% pale gravel and a rarer
    # dark pebble, both keyed off the same deterministic block hash.
    chip = _cell_hash(x, y, 2, seed + 9)
    if chip > 0.986:
        colour = mix_rgb(DIRT_LIGHT, GRAVEL_PALE, (chip - 0.986) / 0.014)
    elif chip < 0.014:
        colour = scaled(DIRT_DARK, 1.0 + chip * 2.0)
    return colour


# ------------------------------------------------------------ grass surface


def _grass_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of short mown night grass: broad patches plus blade fibre."""
    broad = fbm(x, y, SIZE, 3, 8, 21, 811) - 0.5
    clump = tile_noise2(x, y, SIZE, 10, 40, 823) - 0.5
    # Short vertical smear of the per-texel hash: one mown blade stroke.
    fibre = (
        0.55 * (hash01(x, y, 827) - 0.5)
        + 0.30 * (hash01(x, (y - 1) % SIZE, 829) - 0.5)
        + 0.15 * (hash01(x, (y + 1) % SIZE, 831) - 0.5)
    )
    tone = 1.0 + broad * 0.075 + clump * 0.075 + fibre * 0.140
    return (
        GRASS_BASE[0] * tone * (1.0 + 0.020 * clump),
        GRASS_BASE[1] * tone,
        GRASS_BASE[2] * tone * (1.0 - 0.020 * broad),
    )


def build_grass_ground() -> Canvas:
    """Tileable night lawn: mottled short blades in muted green, no edge band.

    The metre-scale variation is two wrapped noise fields and the blade read is
    a per-texel fibre stroke, all stationary over the sheet, so the repeat does
    not expose a quad or a band and the wrapped edges meet mid-lawn.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _grass_rgb(x, y))
    return canvas


# ------------------------------------------------------------- dirt surface


def build_dirt_gravel() -> Canvas:
    """Tileable packed dirt with pale gravel chips, low contrast by design.

    The chips sit on a wrapped 2x2 block field, so the pattern joins its own
    edges exactly; the grain amplitude is deliberately small so a feathered
    decal edge has little tonal step to hide.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _dirt_rgb(x, y, 1009))
    return canvas


# --------------------------------------------------------- concrete surface


def _concrete_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of restrained poured concrete with a few darker pores."""
    broad = fbm(x, y, SIZE, 2, 7, 19, 853) - 0.5
    float_mark = _streak(x, y, 512, 24, 857) - 0.5
    fine = hash01(x, y, 859) - 0.5
    tone = 1.0 + broad * 0.050 + float_mark * 0.030 + fine * 0.050
    colour = scaled(CONCRETE_BASE, tone)
    pore = _cell_hash(x, y, 2, 863)
    if pore > 0.988:
        colour = mix_rgb(colour, CONCRETE_DARK, 0.70)
    elif pore > 0.972:
        colour = mix_rgb(colour, CONCRETE_DARK, 0.25)
    elif pore < 0.008:
        colour = mix_rgb(colour, CONCRETE_LIGHT, 0.50)
    return colour


def build_concrete_pavement() -> Canvas:
    """Tileable pale-grey walkway: subtle panel grain and a few darker pores.

    No expansion joint is painted: at the material's 2 m repeat a joint drawn
    anywhere risks reading as a band, and the brief asks for restrained grain,
    not a grid.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _concrete_rgb(x, y))
    return canvas


# ------------------------------------------------------------- house siding
#
# Eight 64 px clapboards per sheet (15 cm at the material's 1.2 m repeat).  The
# course boundary is phase-offset to y = 32 so the wrapped top/bottom edges
# meet mid-board; each board carries its own tone, a faint horizontal paint
# grain and a shaded top overlap.

SIDING_BOARD = 64
SIDING_PHASE = 32


def _siding_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of horizontal painted clapboard."""
    local = (y - SIDING_PHASE) % SIDING_BOARD
    board = ((y - SIDING_PHASE) % SIZE) // SIDING_BOARD
    tone = 1.0 + (lattice_hash01(board, 0, 877) - 0.5) * 0.050
    tone += (_streak(x, y, 256, 6, 883) - 0.5) * 0.035
    tone += (hash01(x, y, 887 + board) - 0.5) * 0.012
    colour = scaled(SIDING_BASE, tone)
    # The shadow each board casts onto the top of the board it overlaps, plus a
    # narrow lit lip on the protruding bottom edge.
    if local <= 1:
        colour = mix_rgb(colour, SIDING_SHADOW, 0.55)
    elif local <= 3:
        colour = mix_rgb(colour, SIDING_SHADOW, 0.28)
    if local >= SIDING_BOARD - 2:
        colour = mix_rgb(colour, SIDING_LIGHT, 0.32)
    return colour


def build_house_siding() -> Canvas:
    """Tileable muted green-grey clapboard: eight courses, soft overlap shadow.

    A course boundary falls at y = 32 and 96, never on a wrapped edge, so the
    tiling joins mid-board through plain painted wood.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _siding_rgb(x, y))
    return canvas


# ------------------------------------------------------------ roof shingles
#
# Eight 64 px courses (12.5 cm at the material's 1.0 m repeat) with 128 px tabs
# (25 cm).  Courses alternate their joint phase between x = 32 and x = 96, so
# no tab joint ever lands on a wrapped edge; the course boundary at y = 32
# likewise keeps the top/bottom wrap mid-course.

SHINGLE_ROW = 64
SHINGLE_TAB = 128
SHINGLE_PHASE = 32
SHINGLE_TAB_PHASES = (32, 96)


def _shingle_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of dark asphalt shingle: per-tab tone, grain, course shadow."""
    row = ((y - SHINGLE_PHASE) % SIZE) // SHINGLE_ROW
    local_y = (y - SHINGLE_PHASE) % SHINGLE_ROW
    tab_phase = SHINGLE_TAB_PHASES[row % 2]
    tab = ((x - tab_phase) % SIZE) // SHINGLE_TAB
    local_x = (x - tab_phase) % SHINGLE_TAB
    tone = 1.0 + (lattice_hash01(row * 7 + 1, tab * 3 + 1, 907) - 0.5) * 0.070
    tone += (_streak(x, y, 64, 8, 911) - 0.5) * 0.045
    tone += (hash01(x >> 1, y >> 1, 919) - 0.5) * 0.050
    colour = scaled(SHINGLE_BASE, tone)
    if local_y <= 1:
        colour = mix_rgb(colour, SHINGLE_SHADOW, 0.50)
    elif local_y <= 3:
        colour = mix_rgb(colour, SHINGLE_SHADOW, 0.24)
    if local_y >= SHINGLE_ROW - 2:
        colour = mix_rgb(colour, SHINGLE_LIGHT, 0.28)
    if local_x == 0:
        colour = mix_rgb(colour, SHINGLE_SHADOW, 0.35)
    return colour


def build_house_roof_shingle() -> Canvas:
    """Tileable dark asphalt shingles in eight staggered, overlapping courses."""
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _shingle_rgb(x, y))
    return canvas


# ----------------------------------------------------------------- star sky


def build_sky_stars() -> Canvas:
    """Equirectangular 1024x512 night sky: near-black with faint pinpricks.

    The upper half carries :data:`SKY_STAR_COUNT` single- or double-pixel stars
    of varied, deliberately faint brightness; the lower half stays exactly the
    base near-black.  There is no moon, no Milky Way, no glow, no gradient and
    no horizon line.  Stars are inset at least three pixels from every edge, so
    the sheet's u wrap joins through untouched background.
    """
    width, height = 1024, 512
    canvas = Canvas(width, height, fill=(SKY_BASE[0], SKY_BASE[1], SKY_BASE[2], 255))
    rng = _Rng(0x53544152)  # "STAR"

    stars: list[tuple[int, int]] = []
    attempts = 0
    while len(stars) < SKY_STAR_COUNT and attempts < 50_000:
        attempts += 1
        x = 3 + rng.next() % (width - 6)
        y = 3 + rng.next() % 248  # upper half only (the horizon sits at y = 256)
        if any((x - sx) ** 2 + (y - sy) ** 2 < 49 for sx, sy in stars):
            continue
        stars.append((x, y))

    for x, y in stars:
        brightness = SKY_STAR_MIN + rng.next() % SKY_STAR_SPAN
        colour = (
            brightness * 0.93,
            brightness * 0.96,
            min(255.0, brightness * 1.05),
        )
        canvas.set(x, y, colour)
        if brightness >= SKY_BRIGHT_CUTOFF:
            # A second pixel, never a halo: the star stays a pinprick.
            canvas.set(x + 1, y, colour)
            canvas.set(x, y + 1, colour)
            canvas.set(x + 1, y + 1, colour)
    return canvas


# -------------------------------------------------------------- path decals
#
# Each decal paints the shared dirt RGB across the whole sheet and shapes only
# the alpha.  The alpha ramps are continuous functions -- real gradients, never
# a dither pattern -- so the blend pass feathers the edge smoothly.


def build_path_edge() -> Canvas:
    """Path-edge feather, 256x128: dirt fades out along +U.

    Fade contract: the grain is at full alpha along the -U edge (x = 0, the
    path side) and falls smoothly to alpha 0 at the +U sheet edge (x = 255).
    A short taper also fades the two v ends so strips butted end-to-end along a
    long path edge blend instead of showing a hard band.
    """
    width, height = 256, 128
    canvas = Canvas(width, height, fill=DECAL_CLEAR)
    for y in range(height):
        v = y / (height - 1)
        ends = smoothstep(v, 0.0, 0.08) * (1.0 - smoothstep(v, 0.92, 1.0))
        for x in range(width):
            u = x / (width - 1)
            fade = 1.0 - smoothstep(u, 0.30, 0.98)
            canvas.set(x, y, _dirt_rgb(x, y, 1201), 255.0 * fade * ends)
    return canvas


def build_path_end() -> Canvas:
    """Path-end feather, 128x128: dirt fades radially to alpha 0 at every edge.

    The outermost row and column are forced to alpha 0, so however the decal is
    fitted the cut never lands on a visible dirt pixel.
    """
    width = height = 128
    half = width * 0.5
    canvas = Canvas(width, height, fill=DECAL_CLEAR)
    for y in range(height):
        for x in range(width):
            if x in (0, width - 1) or y in (0, height - 1):
                alpha = 0.0
            else:
                dx = x + 0.5 - half
                dy = y + 0.5 - half
                radius = math.hypot(dx, dy) / half
                alpha = 255.0 * (1.0 - smoothstep(radius, 0.30, 1.0))
            canvas.set(x, y, _dirt_rgb(x, y, 1301), alpha)
    return canvas


def build_path_corner() -> Canvas:
    """Path-corner feather, 128x128: two fades meeting at an inner corner.

    Fade contract: the path occupies the bottom and right edges of the sheet
    (an L-shaped run through the corner).  The coverage fades outward along -U
    (leftwards, towards x = 0) and along -V (upwards, towards y = 0); the two
    ramps cross at the path's inner corner near (0.5, 0.5), where a 4-norm soft
    union rounds the fillet instead of leaving a crease.
    """
    width = height = 128
    canvas = Canvas(width, height, fill=DECAL_CLEAR)
    for y in range(height):
        v = (y + 0.5) / height
        fade_v = smoothstep(v, 0.34, 0.66)
        for x in range(width):
            u = (x + 0.5) / width
            fade_u = smoothstep(u, 0.34, 0.66)
            cover = (fade_u ** 4 + fade_v ** 4) ** 0.25
            canvas.set(x, y, _dirt_rgb(x, y, 1409), 255.0 * min(1.0, cover))
    return canvas


# ------------------------------------------------------------------ manifest

ART = {
    "outdoor:tex_grass_ground_01": {
        "model": "environment/outdoor/textures/ground/grass_ground_01.png",
        "build": build_grass_ground,
    },
    "outdoor:tex_dirt_gravel_01": {
        "model": "environment/outdoor/textures/ground/dirt_gravel_01.png",
        "build": build_dirt_gravel,
    },
    "outdoor:tex_concrete_pavement_01": {
        "model": "environment/outdoor/textures/ground/concrete_pavement_01.png",
        "build": build_concrete_pavement,
    },
    "outdoor:tex_house_siding_01": {
        "model": "environment/outdoor/textures/house/siding_01.png",
        "build": build_house_siding,
    },
    "outdoor:tex_house_roof_shingle_01": {
        "model": "environment/outdoor/textures/house/roof_shingle_01.png",
        "build": build_house_roof_shingle,
    },
    "outdoor:tex_sky_stars_01": {
        "model": "environment/outdoor/textures/sky/sky_stars_01.png",
        "build": build_sky_stars,
    },
    "outdoor:decal_path_edge_01": {
        "model": "environment/outdoor/textures/decals/path_edge_01.png",
        "build": build_path_edge,
        "kind": "decal",
    },
    "outdoor:decal_path_end_01": {
        "model": "environment/outdoor/textures/decals/path_end_01.png",
        "build": build_path_end,
        "kind": "decal",
    },
    "outdoor:decal_path_corner_01": {
        "model": "environment/outdoor/textures/decals/path_corner_01.png",
        "build": build_path_corner,
        "kind": "decal",
    },
}


# Committed concept finishes are authoritative; sky remains its original PNG.
from home_art import load_sheet
for _entry in ART.values():
    _model=_entry['model']
    _entry['build']=lambda model=_model: load_sheet(model)

#!/usr/bin/env python3
"""Home surface artwork: wallpaper, paint, trim, floors and ceilings.

The Home set is the clean residential counterpart of the Office and Pool
sheets: new-build wallpaper, painted walls and trim, finished hardwood, cream
carpet and a simple kitchen tile.  Nothing here is aged, stained, damaged or
worn, and no sheet exposes its repeat as a metre checker: the fine tone
variation is stationary noise spread over the whole sheet instead of a handful
of tinted quadrants.

``ART`` maps each logical texture id to its catalog ``model`` path and painter,
which ``build.py`` merges into the manifest.  The PNGs are the authoritative
runtime assets: the game loads them at level load and never runs this script,
and hand-painted replacements are equally valid.

Painting rules for this set:

* 1024x1024, 8-bit RGBA, opaque (alpha 255), tileable in both axes -- every
  drawn pattern period divides the sheet and every noise lookup wraps, so
  ``tools/textures/seam_repair.py --check`` passes on both axes and every
  channel;
* pale, near-neutral albedo for the matte wall and ceiling finishes, and the
  finished wood/floor tones painted at their full albedo (those materials
  author no tint);
* clean by construction: no dirt, stains, water damage, mould, grime, wear or
  damage anywhere in the set;
* structured patterns (the wallpaper motif, the plank and tile grids) are
  phase-offset so a joint never sits on a wrapped sheet edge, and the
  baseboard's darker edge band is a wrapped low-frequency ramp rather than a
  step at the seam;
* everything deterministic -- only :mod:`artkit` helpers, no randomness, no
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

SIZE = 1024

# ------------------------------------------------------------------ palette
#
# Albedo before the baked lighting.  The wall and ceiling bases stay pale and
# near-neutral; the trim and floor woods carry their full colour because their
# materials author no tint.

PAPER_BASE = (243.0, 241.0, 235.0)      # clean off-white printing stock
PAINT_BASE = (245.0, 243.0, 238.0)      # plain painted plaster, a touch cooler
OAK_BASE = (168.0, 130.0, 88.0)         # finished medium oak floorboards
WALNUT_BASE = (118.0, 84.0, 58.0)       # finished walnut floorboards
BASEBOARD_BASE = (170.0, 126.0, 80.0)   # warm oak skirting wood
HANDRAIL_BASE = (148.0, 103.0, 62.0)    # varnished mid-brown handrail
THRESHOLD_BASE = (152.0, 114.0, 74.0)   # transition strip, deeper than the oak
BASEBOARD_WHITE = (238.0, 236.0, 231.0)  # painted white skirting
CARPET_BASE = (208.0, 196.0, 174.0)     # clean cream short-pile carpet
TILE_BASE = (238.0, 236.0, 230.0)       # off-white residential floor tile
TILE_GROUT = (221.0, 219.0, 214.0)      # the 2 px grout line
CEILING_WHITE = (246.0, 245.0, 242.0)   # flat painted ceiling
CEILING_PLASTER = (244.0, 243.0, 239.0)  # fine skim plaster, a shade warmer

# --------------------------------------------------------------- primitives


def _wrap(value: int, period: int) -> int:
    """Positive modulo, so wrapped hash lookups stay inside the sheet."""
    return value % period


def _streak(x: int, y: float, cell_x: int, cell_y: int, seed: int) -> float:
    """Wrapped value noise on an anisotropic lattice: horizontal grain.

    ``cell_x``/``cell_y`` are lattice cell sizes in pixels and must divide
    :data:`SIZE`; a long ``cell_x`` with a short ``cell_y`` stretches every
    feature into a streak along the image x axis.  It is the wood-grain
    primitive: the same smoothstep interpolation :func:`artkit.tile_noise`
    uses, but with independent axis cells, and the lattice index wraps at the
    sheet edges.  ``y`` may be fractional so a slow wobble can meander the
    grain without breaking the wrap.
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
    ix0 = x0 % nx
    ix1 = (x0 + 1) % nx
    iy0 = y0 % ny
    iy1 = (y0 + 1) % ny
    v00 = hash01(ix0, iy0, seed)
    v10 = hash01(ix1, iy0, seed)
    v01 = hash01(ix0, iy1, seed)
    v11 = hash01(ix1, iy1, seed)
    top = v00 + (v10 - v00) * sx
    bottom = v01 + (v11 - v01) * sx
    return top + (bottom - top) * sy


def _wood_tone(
    x: int,
    y: int,
    seed: int,
    warp_amp: float,
    broad_cell: tuple[int, int],
    broad_amp: float,
    fine_cell: tuple[int, int],
    fine_amp: float,
    grain_amp: float = 0.022,
) -> float:
    """One texel of finished wood: meandering streaks over fine grain.

    A broad slow noise wobbles the y coordinate the streaks are read at, so
    the grain meanders like sawn timber instead of running perfectly straight;
    the streaks themselves are two aniso lattice noises (broad and fine) plus
    per-texel grain.  Amplitudes stay in the low percent range: "clear but
    restrained" grain, not a painted tiger stripe.
    """
    warp = (_streak(x, y, 512, 256, seed) - 0.5) * warp_amp
    meander = y + warp
    broad = _streak(x, meander, broad_cell[0], broad_cell[1], seed + 1) - 0.5
    fine = _streak(x, meander, fine_cell[0], fine_cell[1], seed + 2) - 0.5
    grain = hash01(x, y, seed + 3) - 0.5
    return 1.0 + broad * broad_amp + fine * fine_amp + grain * grain_amp


def _wood_canvas(base, seed: int, **grain) -> Canvas:
    """A whole wood sheet from :func:`_wood_tone` and its grain parameters."""
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            tone = _wood_tone(x, y, seed, **grain)
            canvas.set(x, y, (base[0] * tone, base[1] * tone, base[2] * tone))
    return canvas


# ---------------------------------------------------------------- wallpaper


def _paper_rgb(x: int, y: int, patterned: bool) -> tuple[float, float, float]:
    """One texel of the clean paper stock.

    The read is a fine 2 px thread weave (horizontal and vertical smears of the
    per-texel hash) over a gentle low-frequency drift.  There is no motif
    unless ``patterned`` asks for the lattice; the whole variation stays inside
    a few levels around the base so the sheet reads as plain wallpaper.
    """
    low = tile_noise(x, y, SIZE, 3, 21) - 0.5
    weave = (
        0.55 * (hash01(x >> 1, y, 11) - 0.5)
        + 0.30 * (hash01(x, y >> 1, 13) - 0.5)
        + 0.15 * (hash01(x, y, 17) - 0.5)
    )
    mid = tile_noise(x, y, SIZE, 12, 23) - 0.5
    tone = 1.0 + weave * 0.016 + low * 0.007 + mid * 0.004
    if patterned:
        tone *= _wallpaper_motif(x, y)
    return (
        PAPER_BASE[0] * tone * (1.0 + 0.005 * low),
        PAPER_BASE[1] * tone,
        PAPER_BASE[2] * tone * (1.0 - 0.007 * low),
    )


def _paper_canvas(patterned: bool) -> Canvas:
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _paper_rgb(x, y, patterned))
    return canvas


def build_wallpaper_offwhite() -> Canvas:
    """Clean off-white wallpaper: fine paper weave, no pattern at all."""
    return _paper_canvas(patterned=False)


# The motif cell is 128 px = 25 cm at the material's 2 m repeat: eight cells
# across the sheet, so the pattern divides it exactly.
MOTIF_CELL = 128
MOTIF_HALF = MOTIF_CELL // 2


def _wallpaper_motif(x: int, y: int) -> float:
    """ONE understated geometric lattice on a 128 px cell.

    A thin diamond lattice runs through every cell corner and a small
    four-point diamond sits at each cell centre, with a dot where the lattice
    crosses.  The whole motif -- even stacked on the paper's own variation --
    stays inside about ten levels of the base, so it reads as ordinary
    wallcovering at arm's length and never shouts across the room.  Every
    feature is a modulo of the 128 px cell, so the motif wraps with the sheet
    and the lattice corners (not a joint) land on the wrapped edges.
    """
    px = x % MOTIF_CELL
    py = y % MOTIF_CELL
    tone = 1.0

    # Diagonal lattice lines through the cell corners: |px - py| for the "\"
    # family and |px + py - cell| for the "/" family, both wrapped.
    d1 = abs(px - py)
    d1 = min(d1, MOTIF_CELL - d1)
    d2 = abs(px + py - MOTIF_CELL)
    d2 = min(d2, MOTIF_CELL - d2)
    line = min(d1, d2)
    if line <= 1:
        tone *= 0.988
    elif line <= 3:
        tone *= 0.995

    # The diamond outline around the cell centre, a hair darker than the line.
    diamond = abs(px - MOTIF_HALF) + abs(py - MOTIF_HALF)
    if diamond in (17, 18):
        tone *= 0.990
    elif diamond in (15, 16):
        tone *= 0.996

    # A dot at the lattice crossing and a soft highlight in the diamond core.
    if px == 0 and py == 0:
        tone *= 0.986
    if diamond <= 2:
        tone *= 1.008
    return tone


def build_wallpaper_pattern() -> Canvas:
    """The same paper with the one subtle repeating motif."""
    return _paper_canvas(patterned=True)


# ------------------------------------------------------------------- paint
#
# The painted wall must read as a different finish from the wallpaper: no
# directional weave, just roller stipple -- 2 px dimples and a faint long
# horizontal drag -- over an even lower-frequency roller field.


def _paint_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the near-flat painted wall."""
    low = fbm(x, y, SIZE, 2, 6, 13, 37) - 0.5
    pit = hash01(x >> 1, y >> 1, 33)
    stipple = (hash01(x, y, 31) - 0.5) * 0.006
    if pit > 0.78:
        stipple -= 0.006
    elif pit < 0.16:
        stipple += 0.003
    drag = (hash01(x >> 3, y, 35) - 0.5) * 0.004
    tone = 1.0 + stipple + drag + low * 0.008
    return (
        PAINT_BASE[0] * tone,
        PAINT_BASE[1] * tone,
        PAINT_BASE[2] * tone * (1.0 + 0.002 * low),
    )


def build_wall_paint_offwhite() -> Canvas:
    """Plain painted off-white wall: fine roller stipple, no vertical fibre."""
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _paint_rgb(x, y))
    return canvas


# ------------------------------------------------------------------- trim


def build_baseboard_wood() -> Canvas:
    """Residential oak skirting: horizontal grain, a soft darker foot band.

    The band is a raised-cosine ramp centred on the sheet's vertical wrap, so
    the darkest tone sits at the bottom edge of the authored sheet -- the
    floor line where the 9 cm board samples at a 0.5 m repeat -- and the tone
    returns to the board colour smoothly, which keeps the sheet seamless.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        # One full cycle per sheet, cubed to keep the band local: -2 % at the
        # wrap, essentially gone by ~256 px.
        band = (0.5 + 0.5 * math.cos(math.tau * (y + 0.5) / SIZE)) ** 3
        for x in range(SIZE):
            tone = _wood_tone(
                x,
                y,
                311,
                warp_amp=3.0,
                broad_cell=(128, 8),
                broad_amp=0.055,
                fine_cell=(32, 4),
                fine_amp=0.030,
            )
            tone *= 1.0 - 0.020 * band
            canvas.set(
                x,
                y,
                (
                    BASEBOARD_BASE[0] * tone,
                    BASEBOARD_BASE[1] * tone,
                    BASEBOARD_BASE[2] * tone,
                ),
            )
    return canvas


def build_baseboard_white() -> Canvas:
    """Painted white skirting: smooth finish with a fine brush grain.

    The brush is the same horizontal streak primitive at a low amplitude, so
    the board reads as painted wood without any of the oak's figure.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            warp = (_streak(x, y, 512, 256, 401) - 0.5) * 1.5
            brush = _streak(x, y + warp, 256, 8, 402) - 0.5
            fine = hash01(x, y, 403) - 0.5
            tone = 1.0 + brush * 0.010 + fine * 0.006
            canvas.set(
                x,
                y,
                (
                    BASEBOARD_WHITE[0] * tone,
                    BASEBOARD_WHITE[1] * tone,
                    BASEBOARD_WHITE[2] * tone,
                ),
            )
    return canvas


def build_handrail_wood() -> Canvas:
    """Varnished handrail wood: warm mid-brown, fine straight grain.

    The cleanest wood in the set: a long broad streak with little meander and
    a low fine-grain amplitude, which is what a sanded, varnished rail looks
    like next to the looser skirting figure.
    """
    return _wood_canvas(
        HANDRAIL_BASE,
        421,
        warp_amp=2.0,
        broad_cell=(256, 8),
        broad_amp=0.040,
        fine_cell=(64, 4),
        fine_amp=0.018,
        grain_amp=0.016,
    )


def build_threshold_wood() -> Canvas:
    """Transition-strip hardwood: oak a shade deeper, fine grain along x."""
    return _wood_canvas(
        THRESHOLD_BASE,
        433,
        warp_amp=2.5,
        broad_cell=(256, 8),
        broad_amp=0.050,
        fine_cell=(64, 4),
        fine_amp=0.028,
        grain_amp=0.022,
    )


# ------------------------------------------------------------- plank floors
#
# Floorboards run along the image x axis: eight 128 px rows (plank widths) with
# staggered end joints, wrapping plank ends and a 1 px seam at every joint.
# The grids are phase-offset -- the row joints sit at y = 64 mod 128 and the
# per-row end joints are staggered -- so no joint lands on a wrapped edge and
# both wrap directions meet plank interior.

PLANK_PX = 128
PLANK_ROWS = SIZE // PLANK_PX
PLANK_LENGTH = 512
PLANK_PHASE = PLANK_PX // 2


def _plank_floor_rgb(
    x: int,
    y: int,
    base,
    seed: int,
    offsets: tuple[int, ...],
    grain: dict,
) -> tuple[float, float, float]:
    """One texel of a finished plank floor."""
    row = ((y - PLANK_PHASE) % SIZE) // PLANK_PX
    joint = offsets[row]
    plank = ((x - joint) % SIZE) // PLANK_LENGTH
    tone = 1.0 + (hash01(row, plank, seed) - 0.5) * 0.048
    tone *= _wood_tone(x, y, seed + 17 * row + 101 * plank, **grain)
    # The thin seam where the next board begins, plus its lit bevel.
    if y % PLANK_PX == PLANK_PHASE:
        tone *= 0.940
    elif y % PLANK_PX == PLANK_PHASE + 1:
        tone *= 1.010
    if (x - joint) % PLANK_LENGTH == 0:
        tone *= 0.940
    elif (x - joint) % PLANK_LENGTH == 1:
        tone *= 1.010
    return (base[0] * tone, base[1] * tone, base[2] * tone)


def _plank_canvas(base, seed: int, offsets: tuple[int, ...], grain: dict) -> Canvas:
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _plank_floor_rgb(x, y, base, seed, offsets, grain))
    return canvas


# Oak: 20 cm boards at the material's 1.6 m repeat; end joints step 173 px per
# row (0.8 m boards) so the two joints of every row sit at a new phase.
OAK_OFFSETS = tuple(64 + (row * 173) % PLANK_LENGTH for row in range(PLANK_ROWS))
# Walnut: 15 cm boards at 1.2 m; a different stagger -- 0.6 m boards with a
# 101 px step -- and straighter, finer figure so it is a genuinely different
# floor, not a recoloured oak.
WALNUT_OFFSETS = tuple(96 + (row * 101) % PLANK_LENGTH for row in range(PLANK_ROWS))

OAK_GRAIN = {
    "warp_amp": 3.5,
    "broad_cell": (128, 8),
    "broad_amp": 0.060,
    "fine_cell": (32, 4),
    "fine_amp": 0.034,
    "grain_amp": 0.024,
}
WALNUT_GRAIN = {
    "warp_amp": 2.0,
    "broad_cell": (512, 4),
    "broad_amp": 0.050,
    "fine_cell": (128, 4),
    "fine_amp": 0.028,
    "grain_amp": 0.020,
}


def build_hardwood_oak() -> Canvas:
    """Finished oak floorboards: 20 cm planks, 0.8 m boards, staggered."""
    return _plank_canvas(OAK_BASE, 451, OAK_OFFSETS, OAK_GRAIN)


def build_hardwood_walnut() -> Canvas:
    """Finished walnut floorboards: darker, cooler, finer and straighter."""
    return _plank_canvas(WALNUT_BASE, 463, WALNUT_OFFSETS, WALNUT_GRAIN)


# ----------------------------------------------------------------- carpet


def _pile_fibre(x: int, y: int, seed: int) -> float:
    """Short horizontal smear of the per-texel hash: the pile direction."""
    value = 0.50 * hash01(x, y, seed)
    value += 0.30 * hash01(_wrap(x - 1, SIZE), y, seed)
    value += 0.20 * hash01(_wrap(x + 1, SIZE), y, seed)
    return value


def build_carpet_cream() -> Canvas:
    """Clean cream short-pile carpet: fine mottled fibre, low contrast.

    The same construction as the office carpet with every amplitude pulled
    down, no wear pass and no damage: the pile breathes by a couple of levels
    so the floor is alive at close range, but there is no metre-scale blotch
    that could read as a repeating pattern.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            mottle = fbm(x, y, SIZE, 3, 7, 17, 41) - 0.5
            patch = tile_noise(x, y, SIZE, 24, 43) - 0.5
            fibre = _pile_fibre(x, y, 47) - 0.5
            loops = hash01(x >> 1, y, 59) - 0.5
            tuft = hash01(x, y, 53) - 0.5
            tone = (
                1.0
                + 0.030 * mottle
                + 0.025 * patch
                + 0.060 * fibre
                + 0.040 * loops
                + 0.022 * tuft
            )
            canvas.set(
                x,
                y,
                (
                    CARPET_BASE[0] * tone * (1.0 + 0.018 * mottle),
                    CARPET_BASE[1] * tone,
                    CARPET_BASE[2] * tone * (1.0 - 0.026 * mottle),
                ),
            )
    return canvas


# ------------------------------------------------------------------- tile
#
# An 8x8 grid of 128 px tiles (15 cm at the material's 1.2 m repeat) with a
# 2 px grout line.  The grid starts at x = 48, so the joints never land on the
# wrapped sheet edges; the per-tile tone jitter is the only repetition.

TILE_PX = 128
TILE_PHASE = 48
TILE_GROUT_PX = 2


def _tile_home_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the simple residential floor tile."""
    local_x = (x - TILE_PHASE) % TILE_PX
    local_y = (y - TILE_PHASE) % TILE_PX
    tile_x = ((x - TILE_PHASE) % SIZE) // TILE_PX
    tile_y = ((y - TILE_PHASE) % SIZE) // TILE_PX
    if local_x < TILE_GROUT_PX or local_y < TILE_GROUT_PX:
        base = TILE_GROUT
        # The grout is not dead flat: a whisper of body through the joint.
        tone = 1.0 + (hash01(x, y, 107) - 0.5) * 0.012
    else:
        base = TILE_BASE
        tone = 1.0 + (hash01(tile_x, tile_y, 97) - 0.5) * 0.040
        tone *= 1.0 + (hash01(x, y, 101) - 0.5) * 0.008
        field = fbm(x, y, SIZE, 3, 9, 21, 103) - 0.5
        tone *= 1.0 + 0.008 * field
        # The glazed lip just inside the joint, and the soft shadow side.
        if local_x == TILE_GROUT_PX or local_y == TILE_GROUT_PX:
            tone *= 1.006
        if local_x == TILE_PX - 1 or local_y == TILE_PX - 1:
            tone *= 0.994
    return (base[0] * tone, base[1] * tone, base[2] * tone)


def build_tile_home() -> Canvas:
    """Residential floor tile: 15 cm tiles, fine grout, clean and even."""
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, _tile_home_rgb(x, y))
    return canvas


# ---------------------------------------------------------------- ceilings


def build_ceiling_white() -> Canvas:
    """Flat white painted ceiling: faint low-frequency roller variation.

    Only the roller's own broad field is painted; there is no stipple and no
    grid, so the ceiling stays the calmest sheet in the set.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            roller = fbm(x, y, SIZE, 2, 6, 13, 83) - 0.5
            fine = hash01(x, y, 89) - 0.5
            tone = 1.0 + 0.005 * roller + 0.0016 * fine
            canvas.set(
                x,
                y,
                (
                    CEILING_WHITE[0] * tone,
                    CEILING_WHITE[1] * tone,
                    CEILING_WHITE[2] * tone,
                ),
            )
    return canvas


def build_ceiling_plaster() -> Canvas:
    """Fine plaster ceiling: a very light orange-peel stipple, no panel grid.

    Not a popcorn/acoustic texture: the stipple is 1-2 px pits of a couple of
    levels over a faint skim-coat field, well under the office acoustic tile's
    pore contrast.
    """
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            low = fbm(x, y, SIZE, 3, 9, 21, 61) - 0.5
            skim = tile_noise(x, y, SIZE, 48, 67) - 0.5
            stipple = (hash01(x, y, 71) - 0.5) * 0.006
            orange = hash01(x >> 1, y >> 1, 73)
            if orange > 0.80:
                stipple -= 0.005
            elif orange < 0.18:
                stipple += 0.003
            tone = 1.0 + stipple + 0.006 * low + 0.004 * skim
            canvas.set(
                x,
                y,
                (
                    CEILING_PLASTER[0] * tone,
                    CEILING_PLASTER[1] * tone,
                    CEILING_PLASTER[2] * tone,
                ),
            )
    return canvas


# ------------------------------------------------------------------ manifest

ART = {
    "home:tex_wallpaper_offwhite_01": {
        "model": "environment/home/textures/walls/wallpaper_offwhite_01.png",
        "build": build_wallpaper_offwhite,
    },
    "home:tex_wallpaper_pattern_01": {
        "model": "environment/home/textures/walls/wallpaper_pattern_01.png",
        "build": build_wallpaper_pattern,
    },
    "home:tex_wall_paint_offwhite_01": {
        "model": "environment/home/textures/walls/wall_paint_offwhite_01.png",
        "build": build_wall_paint_offwhite,
    },
    "home:tex_baseboard_wood_01": {
        "model": "environment/home/textures/walls/baseboard_wood_01.png",
        "build": build_baseboard_wood,
    },
    "home:tex_baseboard_white_01": {
        "model": "environment/home/textures/walls/baseboard_white_01.png",
        "build": build_baseboard_white,
    },
    "home:tex_handrail_wood_01": {
        "model": "environment/home/textures/walls/handrail_wood_01.png",
        "build": build_handrail_wood,
    },
    "home:tex_threshold_wood_01": {
        "model": "environment/home/textures/floors/threshold_wood_01.png",
        "build": build_threshold_wood,
    },
    "home:tex_hardwood_oak_01": {
        "model": "environment/home/textures/floors/hardwood_oak_01.png",
        "build": build_hardwood_oak,
    },
    "home:tex_hardwood_walnut_02": {
        "model": "environment/home/textures/floors/hardwood_walnut_02.png",
        "build": build_hardwood_walnut,
    },
    "home:tex_carpet_cream_01": {
        "model": "environment/home/textures/floors/carpet_cream_01.png",
        "build": build_carpet_cream,
    },
    "home:tex_tile_home_01": {
        "model": "environment/home/textures/floors/tile_home_01.png",
        "build": build_tile_home,
    },
    "home:tex_ceiling_white_01": {
        "model": "environment/home/textures/ceilings/ceiling_white_01.png",
        "build": build_ceiling_white,
    },
    "home:tex_ceiling_plaster_01": {
        "model": "environment/home/textures/ceilings/ceiling_plaster_01.png",
        "build": build_ceiling_plaster,
    },
}

#!/usr/bin/env python3
"""Pool water artwork: the translucent surface sheet for authored water volumes.

The water surface is a tiling floor-class sheet like every other Pale Pool
surface: square, opaque, seamless in both directions and painted pale because
the material tint and the baked lighting multiply into it.  The read is a
restrained PS1/PS2-era pool: broad soft caustics, a faint directional ripple,
a slow tonal field and a whisper of sparkle -- no photorealism, no foam, no
alpha (the whole sheet stays opaque: the volume's own opacity is a vertex
attribute, not artwork).

``ART`` maps the logical texture id to its catalog ``model`` path and painter;
``build.py`` merges it into the manifest.  The PNG is the authoritative runtime
asset -- the game never runs this script, and the artwork can be replaced by
hand.

Painting rules:

* 1024x1024 (the environment-surface preferred source size), 8-bit RGBA,
  opaque, tileable in both directions (every noise helper wraps and every
  sinusoid completes a whole number of periods across the sheet);
* pale, near-neutral albedo with a faint cool/green cast;
* deterministic -- only :mod:`artkit` helpers and full-period trigonometry, no
  randomness, no clock, no external images.
"""

from __future__ import annotations

import math

from artkit import (
    Canvas,
    clamp,
    hash01,
    mix_rgb,
    smoothstep,
    tile_noise,
)

SIZE = 1024
TAU = 2.0 * math.pi

# ------------------------------------------------------------------ palette
#
# Albedo before the material tint and the baked lighting.  Pool water is pale
# and slightly cool; the sheet never carries the final blue, because the
# material tints it.

WATER_BASE = (206.0, 227.0, 229.0)      # pale near-neutral pool water
CAUSTIC_GLINT = (232.0, 244.0, 244.0)   # the bright crest of a caustic line
TROUGH = (186.0, 210.0, 216.0)          # the slightly deeper tone between webs

# ------------------------------------------------------------------- helpers


def _ridged(x: int, y: int, period: int, seed: int) -> float:
    """One tileable ridged-noise octave in 0..1: soft filaments, not blobs.

    Ridged noise peaks where the wrapped value noise crosses its midpoint, so a
    sum of octaves reads as a loose caustic web rather than as cloudy patches.
    """
    value = tile_noise(x, y, SIZE, period, seed)
    return 1.0 - abs(2.0 * value - 1.0)


def _caustic_web(x: int, y: int) -> float:
    """Three octaves of ridged noise: the caustic web across the 2 m repeat."""
    return (
        0.42 * _ridged(x, y, 6, 401)
        + 0.34 * _ridged(x, y, 16, 409)
        + 0.24 * _ridged(x, y, 44, 419)
    )


def _ripple(x: int, y: int) -> float:
    """A full-period diagonal ripple, phase-warped by wrapped noise.

    The base phase completes eight diagonal periods across the sheet and the
    warp is a wrapped periodic field, so the ripple joins its own edges.
    """
    warp = tile_noise(x, y, SIZE, 3, 431) - 0.5
    phase = TAU * (3.0 * x + 2.0 * y) / SIZE
    return 0.5 + 0.5 * math.sin(phase + 0.8 + 0.9 * warp)


def _field(x: int, y: int) -> float:
    """A broad, slow tonal field across the repeat (2-4 m scale)."""
    return tile_noise(x, y, SIZE, 2, 439)


def water_rgb(x: int, y: int) -> tuple[float, float, float]:
    """One texel of the pool water sheet (a 2 m material repeat)."""
    web = _caustic_web(x, y)
    ripple = _ripple(x, y)
    field = _field(x, y)
    sparkle = hash01(x, y, 443) - 0.5
    # Caustic crests are narrow: the smoothstep keeps them from becoming broad
    # bright patches, and the trough tone fills the space between them.
    crest = smoothstep(web, 0.58, 0.90)
    trough = 1.0 - smoothstep(web, 0.12, 0.48)
    tone = (
        1.0
        + 0.060 * (web - 0.52)
        + 0.035 * (ripple - 0.5)
        + 0.030 * (field - 0.5)
        + 0.012 * sparkle
    )
    r = WATER_BASE[0] * tone
    g = WATER_BASE[1] * tone
    b = WATER_BASE[2] * tone
    # The crest is a touch greener/cooler and the trough a touch deeper; both
    # are gentle mixes, so the sheet still reads as water at a distance.
    mix = clamp(0.42 * crest, 0.0, 0.42)
    r, g, b = mix_rgb((r, g, b), CAUSTIC_GLINT, mix)
    mix = clamp(0.30 * trough, 0.0, 0.30)
    r, g, b = mix_rgb((r, g, b), TROUGH, mix)
    return (r, g, b)


def build_pool_water() -> Canvas:
    canvas = Canvas(SIZE, SIZE)
    for y in range(SIZE):
        for x in range(SIZE):
            canvas.set(x, y, water_rgb(x, y))
    return canvas


# ------------------------------------------------------------------ manifest

ART = {
    "core:tex_pool_water_01": {
        "model": "environment/pool/textures/water/pool_water_01.png",
        "build": build_pool_water,
    },
}

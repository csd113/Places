"""The expanded outdoor kit: birch and conifer trees, the streetlight, the
porch railing family and five house facade families.

Each entry is a ``{"outdoor:<id>": build_function}`` pair following the pack
contract in ``tools/props/README.md``: metres, Y-up, origin on the placement
point the catalogue documents, ``+Z`` facing the player, one embedded
128/256 px atlas per model, and the catalogue's ``size`` matched exactly.

Shared conventions (identical to the first outdoor kit, ``outdoor_props``):

* the house facade kit is a metre grid: a 3.0 m wall module, 0.24 m panel
  depth, a 0.40 m doorway depth, a 3.0 x 1.02 m gable for the kit's 1.75/2.6
  roof pitch, a roof slope whose origin is the eave underside centre and which
  rises towards local ``-Z``, base/pivot at ``y = 0`` and ``+Z`` the front;
* every model is finally fitted to the catalogue box (the same affine measure
  ``outdoor_props._fit_box`` uses for the tree canopy), so the base sits on
  ``y = 0``, the bounding box is horizontally centred and the size is exact;
* one embedded atlas per model. A family's nine pieces share one painted
  sheet, so the five types read as five coherent houses rather than eighteen
  recoloured panels: clapboard vs board-and-batten vs shiplap, five window
  patterns, five trim profiles and five gable/porch treatments.

Families (the catalogue's five display names):

| id | cladding | window | trim | shingles |
| --- | --- | --- | --- | --- |
| 01 | faded cream clapboard | two-pane vertical | dark green | grey |
| 02 | pale blue-grey clapboard | four-pane 2x2 | oxblood | brown |
| 03 | ochre clapboard | tall narrow | white | dark slate |
| 04 | dark green board-and-batten | wide three-lite | cream | near-black |
| 05 | weathered white shiplap | two small squares | navy | green-grey |

Trees carry the level collider note in their prop notes (place them solid with
a trunk-sized box). The streetlight documents its emitter anchor. The porch
railing kit is modular on its 1.8 m grid: the straight's rail runs edge to
edge, the corner wraps a right angle around a ``outdoor:porch_post`` placed on
the module corner, and the end terminates a run with a newel; the fixture in
``tools/levels/build_outdoor_fixture.py`` is the assembled reference.
"""

from __future__ import annotations

import math

import palette
from mesh import FACE_KEYS, PropBuilder
from parts.refreshed import orient_outward, outward_lathe, solid_box, solid_cylinder
from tex import Rng, Texture

# Triangle aims (the pack budget in ``build.py`` is the enforced one; every
# model here stays well inside the 800-triangle review threshold).
TARGETS = {
    "outdoor:tree_02": 262,
    "outdoor:tree_03": 240,
    "outdoor:streetlight": 190,
    "outdoor:porch_railing_straight": 150,
    "outdoor:porch_railing_corner": 100,
    "outdoor:porch_railing_end": 90,
    "outdoor:porch_post": 34,
}

# The facade families: 9 pieces each, 45 models, all inside 500 triangles.
HOUSE_PIECES = (
    "wall_solid",
    "wall_window",
    "wall_doorway",
    "gable",
    "roof_slope",
    "roof_ridge",
    "corner_trim",
    "porch_deck",
    "porch_post",
)
for _piece in HOUSE_PIECES:
    TARGETS[f"outdoor:house_01_{_piece}"] = 320

# --------------------------------------------------------------------- colour
#
# The five house families take their albedo from the catalogue entries. The
# outdoor set stays muted and low-contrast: the trim colours are the only
# saturated notes, and the shingles are close to the siding in value so a
# night-grown house reads as one mass.

BARK_BIRCH = (206, 200, 184)
BARK_BIRCH_FLECK = (54, 50, 46)
BARK_BIRCH_SHADOW = (150, 142, 126)
BARK_CONIFER = (86, 78, 68)
BARK_CONIFER_DARK = (56, 51, 44)
NEEDLE_DARK = (44, 62, 44)
NEEDLE_MID = (58, 79, 55)
NEEDLE_LIGHT = (78, 98, 68)
NEEDLE_VEIL = (34, 46, 34)
BIRCH_LEAF_GREEN = (126, 134, 72)
BIRCH_LEAF_LIGHT = (166, 162, 84)
BIRCH_LEAF_PALE = (188, 178, 104)
BIRCH_VEIL = (74, 76, 48)

STREET_METAL = (63, 60, 54)
STREET_METAL_DARK = (40, 38, 34)
STREET_PANE = (232, 199, 149)
STREET_GLASS = (44, 46, 48)

RAIL_WOOD = (141, 133, 119)
RAIL_WOOD_DARK = (96, 90, 80)
RAIL_METAL = (58, 60, 62)

# The colour behind alpha 0 on a cutout atlas: a muted tone rather than black,
# so mipmaps and bilinear filtering do not fringe the cut edges.
LEAF_VEIL = BIRCH_VEIL

HOUSE_FAMILIES = {
    "01": {
        "name": "Cream Clapboard",
        "cladding": "clapboard",
        "siding": (207, 198, 171),
        "plinth": (126, 122, 112),
        "trim": (63, 74, 58),
        "glass": (42, 47, 54),
        "jamb": (104, 92, 72),
        "shingle": (91, 86, 76),
        "roof_trim": (131, 124, 108),
        "deck": (122, 108, 86),
        "post": (214, 206, 184),
        # (centre x, centre y, width, height, columns, rows)
        "windows": [(-0.40, 1.35, 0.80, 0.94, 2, 1)],
        "boards": 13,
        "board_phase": 0.0,
        "window_dress": "plain",
        "barge": "plain",
        "brackets": 0,
        "ridge_style": "plain",
        "corner_style": "plain",
        "deck_style": "stoop",
        "post_style": "square",
    },
    "02": {
        "name": "Blue Clapboard",
        "cladding": "clapboard",
        "siding": (147, 163, 172),
        "plinth": (118, 112, 104),
        "trim": (109, 47, 42),
        "glass": (38, 42, 50),
        "jamb": (104, 92, 72),
        "shingle": (109, 89, 71),
        "roof_trim": (147, 126, 102),
        "deck": (116, 96, 78),
        "post": (206, 200, 190),
        "windows": [(-0.50, 1.35, 1.00, 1.00, 2, 2)],
        "boards": 13,
        "board_phase": 0.5,
        "window_dress": "pediment",
        "barge": "stepped",
        "brackets": 3,
        "ridge_style": "board",
        "corner_style": "bead",
        "deck_style": "steps2",
        "post_style": "chamfer",
    },
    "03": {
        "name": "Ochre Clapboard",
        "cladding": "clapboard",
        "siding": (200, 160, 90),
        "plinth": (128, 120, 106),
        "trim": (230, 224, 210),
        "glass": (40, 45, 52),
        "jamb": (110, 96, 74),
        "shingle": (61, 66, 73),
        "roof_trim": (120, 124, 130),
        "deck": (126, 112, 88),
        "post": (232, 228, 216),
        "windows": [(-0.25, 1.32, 0.50, 1.30, 1, 2)],
        "boards": 15,
        "board_phase": 0.5,
        "window_dress": "apron",
        "barge": "heavy",
        "brackets": 0,
        "ridge_style": "plain",
        "corner_style": "wide",
        "deck_style": "steps2",
        "post_style": "round",
    },
    "04": {
        "name": "Green Board and Batten",
        "cladding": "battens",
        "siding": (65, 82, 63),
        "plinth": (110, 106, 98),
        "trim": (216, 210, 189),
        "glass": (36, 40, 46),
        "jamb": (102, 90, 70),
        "shingle": (47, 50, 54),
        "roof_trim": (96, 100, 104),
        "deck": (118, 106, 88),
        "post": (218, 212, 192),
        "windows": [(-0.45, 1.35, 1.40, 0.95, 3, 1)],
        "boards": 9,
        "board_phase": 0.0,
        "window_dress": "narrow",
        "barge": "batten",
        "brackets": 3,
        "ridge_style": "plain",
        "corner_style": "batten",
        "deck_style": "small",
        "post_style": "two_stage",
    },
    "05": {
        "name": "White Shiplap",
        "cladding": "shiplap",
        "siding": (217, 214, 204),
        "plinth": (132, 128, 118),
        "trim": (46, 59, 78),
        "glass": (40, 45, 54),
        "jamb": (108, 96, 76),
        "shingle": (89, 100, 92),
        "roof_trim": (128, 134, 126),
        "deck": (124, 110, 90),
        "post": (222, 218, 208),
        "windows": [(-0.95, 1.30, 0.60, 0.60, 1, 1), (0.35, 1.30, 0.60, 0.60, 1, 1)],
        "boards": 17,
        "board_phase": 0.0,
        "window_dress": "bracketed",
        "barge": "plain",
        "brackets": 4,
        "ridge_style": "wide",
        "corner_style": "mitre",
        "deck_style": "deep",
        "post_style": "capital",
    },
}

#: The 256 px family atlas: fractions of the sheet (x, y, width, height).
FAMILY_LAYOUT = {
    "siding": (0.000, 0.000, 1.000, 0.375),
    "trim": (0.000, 0.375, 0.375, 0.250),
    "glass": (0.375, 0.375, 0.250, 0.250),
    "jamb": (0.625, 0.375, 0.375, 0.250),
    "shingle": (0.000, 0.625, 0.500, 0.375),
    "deck": (0.500, 0.625, 0.250, 0.375),
    "post": (0.750, 0.625, 0.250, 0.375),
}


# -------------------------------------------------------------------- helpers


def _tint(color: tuple[int, int, int], lift: float = 0.55) -> tuple[int, int, int]:
    """Vertex colour for a face whose texture is painted in ``color``.

    The shader is ``texture * vertex colour * face shade``; tinting with the
    texture's own colour would multiply it into mud, so the tint is lifted
    towards the pack's light neutral (the same trick as ``parts/furniture``).
    """
    return palette.mix(color, palette.hex_to_rgb(palette.PLASTIC_WHITE), lift)


def _put_rgba(tex: Texture, x: int, y: int, rgb, alpha: int = 255) -> None:
    """Direct RGBA plot: the shared painter forces alpha 255, and a cutout
    atlas needs real transparency, so this module writes its pixels itself."""
    if x < 0 or y < 0 or x >= tex.width or y >= tex.height:
        return
    index = (y * tex.width + x) * 4
    tex.pixels[index] = max(0, min(255, int(round(rgb[0]))))
    tex.pixels[index + 1] = max(0, min(255, int(round(rgb[1]))))
    tex.pixels[index + 2] = max(0, min(255, int(round(rgb[2]))))
    tex.pixels[index + 3] = alpha


def _blob(tex: Texture, centre_x: float, centre_y: float, radius: int, rgb, alpha: int = 255) -> None:
    """One soft-edged pixel blob (a leaf cluster or a needle clump stamp)."""
    r = max(1, int(radius))
    for y in range(int(centre_y) - r, int(centre_y) + r + 1):
        for x in range(int(centre_x) - r, int(centre_x) + r + 1):
            dx = (x - centre_x) / (r + 0.35)
            dy = (y - centre_y) / (r + 0.35)
            if dx * dx + dy * dy <= 1.0:
                _put_rgba(tex, x, y, rgb, alpha)


def _fit_box(mesh, vertex_start: int, target) -> None:
    """Affine-fits every vertex from ``vertex_start`` to an exact box.

    ``target`` is ``((x0, x1), (y0, y1), (z0, z1))``. Geometry is built close
    to the catalogue box and then measured onto it exactly, the same way
    ``Mesh.normalize_origin`` measures the placement origin: the base lands on
    ``y = 0``, the box is centred horizontally and the size is exact.
    """
    positions = mesh.positions[vertex_start:]
    low = [min(point[axis] for point in positions) for axis in range(3)]
    high = [max(point[axis] for point in positions) for axis in range(3)]
    scale = [
        (target[axis][1] - target[axis][0]) / max(1e-6, high[axis] - low[axis])
        for axis in range(3)
    ]
    mesh.positions[vertex_start:] = [
        tuple(
            round(target[axis][0] + (point[axis] - low[axis]) * scale[axis], 6)
            for axis in range(3)
        )
        for point in positions
    ]


def _fit_to_size(p: PropBuilder) -> None:
    """Fits the whole model to the catalogue box (w, h, d) with base at y = 0."""
    width, height, depth = p.size
    _fit_box(p.mesh, 0, ((-width * 0.5, width * 0.5), (0.0, height), (-depth * 0.5, depth * 0.5)))


def _box(p: PropBuilder, center, size, uv, color, shade: bool = True) -> None:
    """Closed outward-facing box; ``uv`` is a rect or a per-face dict."""
    solid_box(p, center, size, uv=uv, color=color, shade=shade)


# ------------------------------------------------------------------- panting


def _paint_birch_bark(tex: Texture, region: str, seed: int) -> None:
    """Pale birch bark: cream base, dark horizontal flecks, soft grey shadow."""
    tex.fill(region, BARK_BIRCH, jitter=6, seed=seed)
    rng = Rng(seed + 1)
    x0, y0, width, height = tex.cell(region)
    for _ in range(26):
        fx = rng.uniform(0.04, 0.92)
        fy = rng.uniform(0.04, 0.96)
        dash = rng.randint(3, 9) * max(1, width // 32)
        thickness = 1 if rng.chance(0.65) else 2
        tone = rng.uniform(0.85, 1.12)
        for step in range(dash):
            for offset in range(thickness):
                _put_rgba(tex, int(x0 + fx * width) + step, int(y0 + fy * height) + offset,
                          palette.shade(BARK_BIRCH_FLECK, tone))
    tex.streaks(region, BARK_BIRCH_SHADOW, count=7, seed=seed + 2, alpha=60)
    tex.streaks(region, palette.shade(BARK_BIRCH, 1.10), count=4, seed=seed + 3, alpha=40)
    tex.grain(region, BARK_BIRCH_SHADOW, seed=seed + 4, density=0.20, alpha=24)
    tex.border(region, BARK_BIRCH_SHADOW, width=1, alpha=40)


def _paint_conifer_bark(tex: Texture, region: str, seed: int) -> None:
    """Rough conifer trunk: dark fissured bark under the needle tiers."""
    tex.fill(region, BARK_CONIFER, jitter=8, seed=seed)
    tex.noise(region, amount=5, freq=3, seed=seed + 1)
    tex.streaks(region, BARK_CONIFER_DARK, count=14, seed=seed + 2, alpha=100)
    tex.streaks(region, palette.shade(BARK_CONIFER, 1.22), count=6, seed=seed + 3, alpha=45)
    tex.grain(region, BARK_CONIFER_DARK, seed=seed + 4, density=0.35, alpha=40)
    tex.border(region, BARK_CONIFER_DARK, width=2, alpha=70)


def _paint_birch_leaf_atlas(tex: Texture, region: str, seed: int) -> None:
    """Alpha-cutout birch foliage: airy clusters of small yellow-green leaves."""
    x0, y0, width, height = tex.cell(region)
    for py in range(y0, y0 + height):
        for px in range(x0, x0 + width):
            _put_rgba(tex, px, py, BIRCH_VEIL, 0)
    rng = Rng(seed)
    greens = (BIRCH_LEAF_GREEN, BIRCH_LEAF_LIGHT, BIRCH_LEAF_PALE, (140, 148, 80))
    # Airy: more, smaller clusters with wider transparent gaps than tree_01.
    for _ in range(16):
        cx = x0 + width * rng.uniform(0.10, 0.90)
        cy = y0 + height * rng.uniform(0.10, 0.90)
        for _ in range(rng.randint(5, 9)):
            spread = rng.uniform(0.05, 0.18) * width
            bx = cx + rng.uniform(-spread, spread)
            by = cy + rng.uniform(-spread, spread)
            _blob(tex, bx, by, rng.randint(2, 5), palette.shade(rng.pick(greens), rng.uniform(0.86, 1.14)))
    for _ in range(26):
        bx = x0 + width * rng.uniform(0.03, 0.97)
        by = y0 + height * rng.uniform(0.03, 0.97)
        _blob(tex, bx, by, rng.randint(1, 3), palette.shade(rng.pick(greens), rng.uniform(0.85, 1.12)))


def _paint_needle_atlas(tex: Texture, region: str, seed: int, *, cutout: bool) -> None:
    """Evergreen needles: dense short strokes; ``cutout`` keeps gaps transparent."""
    x0, y0, width, height = tex.cell(region)
    if cutout:
        for py in range(y0, y0 + height):
            for px in range(x0, x0 + width):
                _put_rgba(tex, px, py, NEEDLE_VEIL, 0)
    else:
        tex.fill(region, NEEDLE_MID, jitter=6, seed=seed)
    rng = Rng(seed + 1)
    strokes = 240 if cutout else 420
    tones = (NEEDLE_DARK, NEEDLE_MID, NEEDLE_LIGHT, (92, 110, 74))
    for _ in range(strokes):
        bx = x0 + rng.uniform(0.0, 1.0) * width
        by = y0 + rng.uniform(0.0, 1.0) * height
        length = rng.uniform(0.08, 0.26) * height
        lean = rng.uniform(-0.18, 0.18) * width
        r = max(1, int(round(rng.uniform(1.0, 2.6))))
        tone = palette.shade(rng.pick(tones), rng.uniform(0.82, 1.16))
        steps = max(2, int(length))
        for step in range(steps):
            t = step / steps
            _blob(tex, bx + lean * t, by - length * t, r, tone, 255)


def _paint_metal_housing(tex: Texture, region: str, base, seed: int) -> None:
    """Dark weathered painted metal: brushed grain, rust freckles, grime."""
    dark = palette.shade(base, 0.60)
    light = palette.shade(base, 1.30)
    tex.fill(region, base, jitter=7, seed=seed)
    tex.noise(region, amount=4, freq=4, seed=seed + 1)
    tex.grain(region, dark, seed=seed + 2, density=0.35, alpha=45)
    tex.grain(region, light, seed=seed + 3, density=0.25, alpha=26)
    tex.streaks(region, dark, count=4, seed=seed + 4, alpha=28)
    tex.spots(region, palette.hex_to_rgb(palette.RUST), count=4, seed=seed + 5, radius=2, alpha=34)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 6, radius=2, alpha=30)
    tex.border(region, dark, width=1, alpha=60)


def _paint_warm_pane(tex: Texture, region: str, seed: int) -> None:
    """The family's warm glass: a soft amber gradient with a hot centre."""
    tex.gradient(region, palette.shade(STREET_PANE, 1.16), palette.shade(STREET_PANE, 0.82), jitter=3, seed=seed)
    tex.spots(region, palette.shade(STREET_PANE, 1.32), count=6, seed=seed + 1, radius=3, alpha=26)
    tex.spots(region, palette.shade(STREET_PANE, 0.74), count=4, seed=seed + 2, radius=2, alpha=20)
    tex.border(region, palette.shade(STREET_PANE, 0.55), width=2, alpha=110)


def _paint_rail_wood(tex: Texture, region: str, base, seed: int) -> None:
    """Painted porch timber: vertical grain, a dusty highlight, light wear."""
    dark = palette.shade(base, 0.62)
    light = palette.shade(base, 1.20)
    tex.fill(region, base, jitter=7, seed=seed)
    tex.noise(region, amount=4, freq=3, seed=seed + 1)
    tex.streaks(region, dark, count=10, seed=seed + 2, alpha=70)
    tex.streaks(region, light, count=5, seed=seed + 3, alpha=36)
    tex.grain(region, dark, seed=seed + 4, density=0.32, alpha=34)
    tex.spots(region, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 5, radius=2, alpha=22)
    tex.border(region, dark, width=1, alpha=50)


def _paint_rail_metal(tex: Texture, region: str, seed: int) -> None:
    """Painted metal balusters: flat, a faint lengthwise brush, dark edges."""
    dark = palette.shade(RAIL_METAL, 0.66)
    tex.fill(region, RAIL_METAL, jitter=5, seed=seed)
    tex.grain(region, dark, seed=seed + 1, density=0.30, alpha=40)
    tex.grain(region, palette.shade(RAIL_METAL, 1.28), seed=seed + 2, density=0.20, alpha=22)
    tex.spots(region, palette.hex_to_rgb(palette.RUST), count=2, seed=seed + 3, radius=1, alpha=22)
    tex.border(region, dark, width=1, alpha=55)


# ---------------------------------------------------------- house family art


def _region_rect(name: str, size: int) -> tuple[int, int, int, int]:
    fx, fy, fw, fh = FAMILY_LAYOUT[name]
    return (
        int(round(fx * size)),
        int(round(fy * size)),
        max(1, int(round(fw * size))),
        max(1, int(round(fh * size))),
    )


def _paint_clapboard(tex: Texture, name: str, family: dict, seed: int) -> None:
    """Horizontal painted clapboard over the type's plinth band.

    ``family['boards']`` courses run the region's V axis (the wall's full
    height); each course carries its own tone, an overlap shadow line and a
    narrow lit lip. The plinth is painted as the bottom 0.35 m of the panel.
    """
    base = family["siding"]
    dark = palette.shade(base, 0.72)
    light = palette.shade(base, 1.14)
    boards = int(family["boards"])
    tex.fill(name, base, jitter=5, seed=seed)
    for index in range(boards):
        v0 = index / boards
        v1 = (index + 1) / boards
        tex.band(name, palette.shade(base, 1.0 + 0.035 * ((index + 1) % 2)), v0, v1)
        tex.band(name, dark, v0, v0 + 0.012, alpha=150)
        tex.band(name, light, v1 - 0.012, v1, alpha=70)
    tex.noise(name, amount=4, freq=3, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.26, alpha=26)
    tex.grain(name, light, seed=seed + 3, density=0.18, alpha=18)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 4, radius=3, alpha=18)
    band_top = 1.0 - 0.35 / 2.664
    tex.band(name, family["plinth"], band_top, 1.0)
    tex.band(name, palette.shade(family["plinth"], 0.68), band_top, band_top + 0.016, alpha=170)
    tex.spots(name, palette.shade(family["plinth"], 0.86), count=5, seed=seed + 5, radius=3, alpha=26)
    tex.border(name, dark, width=1, alpha=45)


def _paint_shiplap(tex: Texture, name: str, family: dict, seed: int) -> None:
    """Weathered horizontal shiplap: narrow boards with a rabbit groove."""
    base = family["siding"]
    dark = palette.shade(base, 0.70)
    light = palette.shade(base, 1.10)
    boards = int(family["boards"])
    tex.fill(name, base, jitter=5, seed=seed)
    for index in range(boards):
        v0 = index / boards
        v1 = (index + 1) / boards
        tex.band(name, palette.shade(base, 0.97 + 0.05 * (index % 3)), v0, v1)
        # The rabbit: a dark groove line one pixel in from each board's top,
        # with a soft lit shoulder under it (shiplap rides flush, no overlap).
        tex.band(name, dark, v0 + 0.010, v0 + 0.024, alpha=160)
        tex.band(name, light, v0 + 0.026, v0 + 0.040, alpha=60)
    tex.noise(name, amount=4, freq=3, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.24, alpha=26)
    tex.grain(name, light, seed=seed + 3, density=0.16, alpha=16)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=5, seed=seed + 4, radius=3, alpha=20)
    band_top = 1.0 - 0.35 / 2.664
    tex.band(name, family["plinth"], band_top, 1.0)
    tex.band(name, palette.shade(family["plinth"], 0.70), band_top, band_top + 0.016, alpha=170)
    tex.spots(name, palette.shade(family["plinth"], 0.88), count=5, seed=seed + 5, radius=3, alpha=24)
    tex.border(name, dark, width=1, alpha=45)


def _paint_battens(tex: Texture, name: str, family: dict, seed: int) -> None:
    """Board-and-batten cladding: vertical boards with applied batten strips."""
    base = family["siding"]
    dark = palette.shade(base, 0.66)
    light = palette.shade(base, 1.18)
    tex.fill(name, base, jitter=6, seed=seed)
    x0, y0, width, height = tex.cell(name)
    battens = max(4, int(family["boards"]) - 1)
    for index in range(battens + 1):
        # Board joints: a fine shadow line between vertical boards.
        u = index / (battens + 1)
        px = x0 + int(round(u * width))
        for py in range(y0, y0 + height):
            _put_rgba(tex, px, py, dark, 255)
    for index in range(battens):
        fx = (index + 0.5) / battens
        px = x0 + int(round(fx * width))
        half = max(1, width // 42)
        for py in range(y0, y0 + height):
            for offset in range(-half, half + 1):
                tone = 1.0 + (0.10 if offset < 0 else -0.16 if offset >= half else 0.0)
                _put_rgba(tex, px + offset, py, palette.shade(base, tone), 255)
    tex.noise(name, amount=4, freq=4, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.22, alpha=24)
    tex.grain(name, light, seed=seed + 3, density=0.16, alpha=16)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 4, radius=3, alpha=20)
    band_top = 1.0 - 0.35 / 2.664
    tex.band(name, family["plinth"], band_top, 1.0)
    tex.band(name, palette.shade(family["plinth"], 0.70), band_top, band_top + 0.016, alpha=170)
    tex.spots(name, palette.shade(family["plinth"], 0.88), count=4, seed=seed + 5, radius=3, alpha=24)
    tex.border(name, dark, width=1, alpha=45)


def _paint_trim_wood(tex: Texture, name: str, base, seed: int) -> None:
    """Painted trim board: flat, a whisper of brush grain, soft edges."""
    dark = palette.shade(base, 0.74)
    tex.fill(name, base, jitter=4, seed=seed)
    tex.noise(name, amount=3, freq=4, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.18, alpha=20)
    tex.grain(name, palette.shade(base, 1.09), seed=seed + 3, density=0.14, alpha=14)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=2, seed=seed + 4, radius=2, alpha=14)
    tex.border(name, dark, width=1, alpha=38)


def _paint_glazing(tex: Texture, name: str, base, seed: int) -> None:
    """Opaque dark glazing with one faint sheen; never transparent."""
    dark = palette.shade(base, 0.72)
    bright = palette.shade(base, 1.55)
    tex.gradient(name, palette.shade(base, 1.24), dark, jitter=3, seed=seed)
    tex.bar(name, bright, (0.10, 0.04, 0.30, 0.94), alpha=18)
    tex.bar(name, palette.shade(base, 0.50), (0.62, 0.0, 0.80, 1.0), alpha=22)
    tex.border(name, palette.shade(base, 0.46), width=2, alpha=120)


def _paint_jamb_wood(tex: Texture, name: str, base, seed: int) -> None:
    """Door and window reveal timber: end-grain pale, a little grime."""
    dark = palette.shade(base, 0.68)
    tex.fill(name, base, jitter=6, seed=seed)
    tex.noise(name, amount=4, freq=3, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.28, alpha=30)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 3, radius=2, alpha=22)
    tex.border(name, dark, width=1, alpha=50)


def _paint_shingles(tex: Texture, name: str, base, seed: int, rows: int, cols: int) -> None:
    """Overlapping shingle tabs: horizontal courses with staggered joints."""
    dark = palette.shade(base, 0.60)
    light = palette.shade(base, 1.20)
    tex.fill(name, base, jitter=7, seed=seed)
    tex.noise(name, amount=5, freq=3, seed=seed + 1)
    for row in range(rows):
        v0 = row / rows
        v1 = (row + 1) / rows
        tex.band(name, palette.shade(base, 0.92 + 0.06 * (row % 2)), v0 + 0.012, v1, alpha=200)
        tex.band(name, dark, v0, v0 + 0.020, alpha=190)
        tex.band(name, light, v0 + 0.020, v0 + 0.036, alpha=40)
        for column in range(cols):
            u = (column + 0.5 * (row % 2)) / cols
            tex.bar(name, dark, (u, v0 + 0.012, u + 0.010, v1), alpha=110)
    tex.streaks(name, palette.hex_to_rgb(palette.GRIME), count=4, seed=seed + 2, alpha=24)
    tex.spots(name, palette.shade(base, 0.68), count=5, seed=seed + 3, radius=2, alpha=24)
    tex.border(name, dark, width=1, alpha=60)


def _paint_deck_boards(tex: Texture, name: str, base, seed: int, boards: int) -> None:
    """Porch decking: boards with a shadowed gap between them and grain."""
    dark = palette.shade(base, 0.62)
    light = palette.shade(base, 1.16)
    tex.fill(name, base, jitter=6, seed=seed)
    for index in range(boards):
        u0 = index / boards
        u1 = (index + 1) / boards
        tex.bar(name, palette.shade(base, 0.94 + 0.05 * (index % 3)), (u0 + 0.008, 0.0, u1 - 0.008, 1.0))
        tex.bar(name, dark, (u0, 0.0, u0 + 0.008, 1.0), alpha=180)
        tex.bar(name, light, (u0 + 0.008, 0.0, u0 + 0.016, 1.0), alpha=60)
    tex.noise(name, amount=4, freq=4, seed=seed + 1)
    tex.grain(name, dark, seed=seed + 2, density=0.24, alpha=28)
    tex.spots(name, palette.hex_to_rgb(palette.GRIME), count=3, seed=seed + 3, radius=2, alpha=20)
    tex.border(name, dark, width=1, alpha=50)


def _paint_post_timber(tex: Texture, name: str, base, seed: int) -> None:
    """Painted porch post: vertical brush grain and a darker base shadow."""
    dark = palette.shade(base, 0.70)
    light = palette.shade(base, 1.12)
    tex.fill(name, base, jitter=5, seed=seed)
    tex.noise(name, amount=3, freq=4, seed=seed + 1)
    tex.streaks(name, dark, count=8, seed=seed + 2, alpha=50)
    tex.streaks(name, light, count=4, seed=seed + 3, alpha=28)
    tex.grain(name, dark, seed=seed + 4, density=0.26, alpha=26)
    tex.border(name, dark, width=1, alpha=45)


def _family_atlas(p: PropBuilder, family: dict, size: int, names) -> Texture:
    """Paints one family's shared sheet; only the requested regions are painted."""
    tex = p.set_texture(size, seed=4701)
    for region in names:
        tex.region(region, _region_rect(region, size))
    if "siding" in names:
        painter = {
            "clapboard": _paint_clapboard,
            "shiplap": _paint_shiplap,
            "battens": _paint_battens,
        }[family["cladding"]]
        painter(tex, "siding", family, seed=4601)
    if "trim" in names:
        _paint_trim_wood(tex, "trim", family["trim"], seed=4611)
    if "glass" in names:
        _paint_glazing(tex, "glass", family["glass"], seed=4621)
    if "jamb" in names:
        _paint_jamb_wood(tex, "jamb", family["jamb"], seed=4631)
    if "shingle" in names:
        rows, cols = (8, 7) if family["cladding"] != "battens" else (9, 5)
        _paint_shingles(tex, "shingle", family["shingle"], seed=4641, rows=rows, cols=cols)
    if "deck" in names:
        _paint_deck_boards(tex, "deck", family["deck"], seed=4651, boards=9)
    if "post" in names:
        _paint_post_timber(tex, "post", family["post"], seed=4661)
    return tex


# ------------------------------------------------------------------- trees


def _normalize(vector):
    length = math.sqrt(sum(value * value for value in vector))
    if length < 1e-9:
        return (0.0, 0.0, 1.0)
    return tuple(value / length for value in vector)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _leaf_card(p: PropBuilder, centre, normal, width: float, height: float, uv,
               color, shade_mult: float, cols: int = 2, rows: int = 2, bow: float = 0.05) -> None:
    """One alpha-cutout leaf card: a bowed grid facing ``normal``."""
    n = _normalize(normal)
    reference = (1.0, 0.0, 0.0) if abs(n[1]) > 0.94 else (0.0, 1.0, 0.0)
    right = _normalize(_cross(reference, n))
    up = _cross(n, right)
    u0, v0, u1, v1 = uv
    for column in range(cols):
        for row in range(rows):
            fu0, fu1 = column / cols, (column + 1) / cols
            fv0, fv1 = row / rows, (row + 1) / rows
            corners = []
            uvs = []
            for fu, fv in ((fu0, fv0), (fu1, fv0), (fu1, fv1), (fu0, fv1)):
                bulge = bow * math.sin(math.pi * fu) * math.sin(math.pi * fv)
                point = tuple(
                    centre[axis] + right[axis] * (fu - 0.5) * width + up[axis] * (fv - 0.5) * height
                    + n[axis] * bulge
                    for axis in range(3)
                )
                corners.append(point)
                uvs.append((u0 + fu * (u1 - u0), v1 - fv * (v1 - v0)))
            p.mesh.quad(*corners, uv=uvs, color=color, shade_mult=shade_mult)


def build_tree_02(p: PropBuilder) -> None:
    """Birch: slender pale-flecked trunk, thin branches, an airy pale canopy.

    The trunk is 0.26 m across at the floor (inside the documented 0.22-0.30
    range), so place the prop solid with a level size of roughly
    ``[0.6, 6.2, 0.6]`` and only the trunk blocks the player. The canopy is
    alpha-cutout leaf cards (``MASK``, cutoff 0.5) fitted to the 3.6 x 3.6 m
    footprint; its leaves sit higher and are sparser and yellower than
    ``outdoor:tree_01``.
    """
    tex = p.set_texture(256, seed=7701, alpha=True)
    tex.region("trunk", (0, 0, 128, 128))
    tex.region("branch", (128, 0, 128, 128))
    tex.region("leaf", (0, 128, 256, 128))
    _paint_birch_bark(tex, "trunk", seed=7703)
    _paint_birch_bark(tex, "branch", seed=7709)
    _paint_birch_leaf_atlas(tex, "leaf", seed=7717)
    trunk_uv = tex.uv("trunk", inset=2)
    branch_uv = tex.uv("branch", inset=2)
    leaf_uv = tex.uv("leaf", inset=2)

    trunk_slot = p.material("birch_trunk")
    branch_slot = p.material("birch_branch")
    leaf_slot = p.material("birch_leaves", alpha_mode="mask")

    p.begin_material(trunk_slot)
    profile = (
        (0.00, 0.130),
        (0.55, 0.104),
        (1.70, 0.082),
        (2.90, 0.062),
        (3.90, 0.046),
        (4.60, 0.032),
    )
    start = len(p.mesh.indices)
    p.mesh.lathe((0.0, 0.0, 0.0), profile, segments=6, axis="y", uv=trunk_uv,
                 color=_tint(BARK_BIRCH, 0.50))
    orient_outward(p.mesh, start, (0.0, 2.0, 0.0))

    p.begin_material(branch_slot)
    for azimuth, base_y, base_offset, base_radius, reach, rise, tip_radius in (
        (35.0, 2.70, 0.03, 0.038, 1.20, 1.55, 0.015),
        (128.0, 3.05, 0.03, 0.034, 1.30, 1.30, 0.013),
        (215.0, 2.85, 0.03, 0.036, 1.15, 1.60, 0.014),
        (300.0, 3.20, 0.03, 0.032, 1.25, 1.25, 0.012),
    ):
        angle = math.radians(azimuth)
        direction = (math.cos(angle), 0.0, math.sin(angle))
        base = (direction[0] * base_offset, base_y, direction[2] * base_offset)
        tip = (direction[0] * reach, base_y + rise, direction[2] * reach)
        mid = (
            base[0] + (tip[0] - base[0]) * 0.55,
            base[1] + (tip[1] - base[1]) * 0.55 + 0.08,
            base[2] + (tip[2] - base[2]) * 0.55,
        )
        p.tube_path([base, mid, tip], radii=[base_radius, base_radius * 0.6, tip_radius],
                    segments=5, uv=branch_uv, color=_tint(BARK_BIRCH_SHADOW, 0.45),
                    cap_start=True, cap_end=True)

    p.begin_material(leaf_slot)
    vertex_start = len(p.mesh.positions)
    rng = Rng(0xB17C)
    centre = (0.0, 4.25, 0.0)
    radius = 1.25
    for elevation, count, yaw_offset, card_w, card_h in (
        (68.0, 3, 40.0, 1.05, 1.00),
        (42.0, 4, 12.0, 1.30, 1.15),
        (5.0, 4, 45.0, 1.35, 1.15),
        (-40.0, 3, 0.0, 1.15, 0.95),
    ):
        phi = math.radians(elevation)
        for index in range(count):
            yaw = math.radians(yaw_offset + 360.0 * index / count)
            radial = (math.cos(phi) * math.cos(yaw), math.sin(phi), math.cos(phi) * math.sin(yaw))
            card_centre = tuple(centre[axis] + radius * radial[axis] for axis in range(3))
            _leaf_card(p, card_centre, radial, card_w, card_h, leaf_uv,
                       (250, 250, 240), 0.92 + 0.05 * rng.randint(0, 3), cols=2, rows=2)
    for elevation, card_w, card_h in ((90.0, 1.00, 1.00), (-90.0, 1.10, 1.10)):
        card_centre = (0.0, centre[1] + radius * math.sin(math.radians(elevation)), 0.0)
        normal = (0.0, math.sin(math.radians(elevation)), 0.0)
        _leaf_card(p, card_centre, normal, card_w, card_h, leaf_uv, (240, 242, 232), 0.90)
    _fit_box(p.mesh, vertex_start, ((-1.8, 1.8), (2.30, 6.2), (-1.8, 1.8)))
    _fit_to_size(p)

    p.add_note("birch: pale bark with dark flecks, thin branches, airy pale canopy")
    p.add_note("trunk is 0.26 m across at the floor and 6 segments; base at y=0, horizontally centred")
    p.add_note("place it solid with a level size of roughly [0.6, 6.2, 0.6] so only the trunk blocks the player")
    p.add_note("canopy is alpha-cutout leaf cards (MASK, cutoff 0.5) fitted to the 3.6 x 3.6 m footprint")


def _cone_tier(p: PropBuilder, base_y: float, base_radius: float, top_y: float, top_radius: float,
               uv, color, seed: int, segments: int = 8, jitter: float = 0.12) -> None:
    """One tier of an evergreen: a jittered cone with a base skirt and top disc.

    The radius of every base and top ring vertex is scaled by a deterministic
    factor, so the silhouette is ragged rather than a clean cone. The whole
    tier is a convex-ish shell and is re-oriented outward from its axis.
    """
    rng = Rng(seed)
    base_ring = []
    top_ring = []
    for index in range(segments):
        angle = math.tau * index / segments
        radius = base_radius * (1.0 + rng.uniform(-jitter, jitter * 1.15))
        y = base_y + rng.uniform(-0.03, 0.03) * base_radius
        base_ring.append((math.cos(angle) * radius, y, math.sin(angle) * radius))
        top_ring.append((
            math.cos(angle) * top_radius * (1.0 + rng.uniform(-0.2, 0.2)),
            top_y + rng.uniform(-0.02, 0.02),
            math.sin(angle) * top_radius * (1.0 + rng.uniform(-0.2, 0.2)),
        ))
    start = len(p.mesh.indices)
    u0, v0, u1, v1 = uv
    for index in range(segments):
        nxt = (index + 1) % segments
        p.mesh.quad(
            base_ring[index], base_ring[nxt], top_ring[nxt], top_ring[index],
            uv=[(u0 + (u1 - u0) * (index / segments), v1),
                (u0 + (u1 - u0) * ((index + 1) / segments), v1),
                (u0 + (u1 - u0) * ((index + 1) / segments), v0),
                (u0 + (u1 - u0) * (index / segments), v0)],
            color=color, shade_mult=0.78 + 0.22 * math.cos(math.tau * (index + 0.5) / segments - 0.9),
        )
    for index in range(segments):
        nxt = (index + 1) % segments
        p.mesh.triangle((0.0, base_y + 0.02, 0.0), base_ring[nxt], base_ring[index],
                        _cap_uvs(uv), palette.shade(color, 0.7), shade_mult=0.7)
        p.mesh.triangle((0.0, top_y, 0.0), top_ring[index], top_ring[nxt],
                        _cap_uvs(uv), color, shade_mult=0.95)
    orient_outward(p.mesh, start, (0.0, (base_y + top_y) * 0.5, 0.0))


def _cap_uvs(rect):
    u0, v0, u1, v1 = rect
    return [(u0 + (u1 - u0) * 0.5, v0 + (v1 - v0) * 0.5), (u0, v1), (u1, v1)]


def build_tree_03(p: PropBuilder) -> None:
    """Conifer: a tapered trunk under five to six stacked ragged evergreen tiers.

    Place it solid with a level size of roughly ``[0.6, 6.8, 0.6]`` so only the
    trunk blocks the player. The tiers are jittered cones sharing one needle
    atlas; a few alpha-cutout needle-fringe cards break the silhouette.
    """
    tex = p.set_texture(256, seed=7801, alpha=True)
    tex.region("bark", (0, 0, 128, 128))
    tex.region("needle", (128, 0, 128, 128))
    tex.region("fringe", (0, 128, 256, 128))
    _paint_conifer_bark(tex, "bark", seed=7803)
    _paint_needle_atlas(tex, "needle", seed=7811, cutout=False)
    _paint_needle_atlas(tex, "fringe", seed=7823, cutout=True)
    bark_uv = tex.uv("bark", inset=2)
    needle_uv = tex.uv("needle", inset=2)
    fringe_uv = tex.uv("fringe", inset=2)

    bark_slot = p.material("conifer_trunk")
    needle_slot = p.material("conifer_needles")
    fringe_slot = p.material("conifer_fringe", alpha_mode="mask")

    p.begin_material(bark_slot)
    start = len(p.mesh.indices)
    p.mesh.lathe((0.0, 0.0, 0.0),
                 ((0.0, 0.120), (0.60, 0.092), (1.80, 0.072), (3.20, 0.056),
                  (4.60, 0.040), (5.60, 0.028)),
                 segments=6, axis="y", uv=bark_uv, color=_tint(BARK_CONIFER, 0.40))
    orient_outward(p.mesh, start, (0.0, 2.4, 0.0))

    p.begin_material(needle_slot)
    for index, (base_y, base_radius, top_y, top_radius) in enumerate((
        (0.42, 1.52, 1.95, 0.42),
        (1.35, 1.40, 2.85, 0.36),
        (2.25, 1.24, 3.75, 0.30),
        (3.15, 1.04, 4.65, 0.24),
        (4.05, 0.82, 5.45, 0.18),
        (4.85, 0.58, 6.45, 0.10),
    )):
        _cone_tier(p, base_y, base_radius, top_y, top_radius, needle_uv,
                   _tint(NEEDLE_MID, 0.30), seed=7831 + index * 7)

    p.begin_material(fringe_slot)
    rng = Rng(0xC0FF)
    for index in range(8):
        yaw = math.radians(24.0 + 45.0 * index)
        height = rng.uniform(0.85, 1.15)
        radius = 1.62 - 0.16 * (index % 3)
        centre = (math.cos(yaw) * radius, rng.uniform(1.05, 2.65), math.sin(yaw) * radius)
        normal = (math.cos(yaw), 0.55, math.sin(yaw))
        _leaf_card(p, centre, normal, rng.uniform(0.55, 0.80), height, fringe_uv,
                   (240, 244, 236), 0.92 + 0.05 * (index % 3), cols=1, rows=2, bow=0.03)
    _fit_to_size(p)

    p.add_note("conifer: tapered trunk under six stacked ragged needle tiers")
    p.add_note("trunk is 0.24 m across at the floor; base at y=0, horizontally centred")
    p.add_note("place it solid with a level size of roughly [0.6, 6.8, 0.6] so only the trunk blocks the player")
    p.add_note("needle-fringe cards are alpha-cutout (MASK, cutoff 0.5) and carry no collision")


# ------------------------------------------------------------- streetlight
#
# A 6 m cast-metal post with a rear footing, a swan-neck arm reaching +Z and a
# downward lantern: the head is a shell open at the bottom, a hood that
# overhangs it, and a pane quad facing -Y as its own emissive material. The
# level's point light sits just below the pane; see the placement notes.


def build_streetlight(p: PropBuilder) -> None:
    """Streetlight: base plate, 6 m post, +Z arm and a downward lantern head.

    The pane faces -Y at local ``(0, 6.08, 0.43)`` and is its own emissive
    material; ``p.add_note`` carries the suggested light and the anchor
    caveat. The base plate reaches to ``z = -0.60`` as the rear footing, so
    the 1.2 m deep, bounding-box-centred model has its post just forward of
    centre and the lantern at the +Z extreme.
    """
    tex = p.set_texture(128, seed=7901)
    tex.region("metal", (0, 0, 128, 64))
    tex.region("pane", (0, 64, 128, 64))
    _paint_metal_housing(tex, "metal", STREET_METAL, seed=7903)
    _paint_warm_pane(tex, "pane", seed=7913)
    metal_uv = tex.uv("metal", inset=2)
    pane_uv = tex.uv("pane", inset=2)
    metal_slot = p.material("street_metal")
    pane_slot = p.material("street_pane", emissive=(1.0, 0.82, 0.60), strength=1.35)

    p.begin_material(metal_slot)
    base = _tint(STREET_METAL, 0.34)
    cap_color = _tint(STREET_METAL, 0.42)
    # Footing: a long plate running back from the post plus a chamfered pad.
    _box(p, (0.0, 0.050, -0.25), (0.50, 0.10, 0.70), metal_uv, base)
    _box(p, (0.0, 0.135, -0.22), (0.34, 0.07, 0.50), metal_uv, _tint(STREET_METAL, 0.40))
    _box(p, (0.0, 0.105, 0.02), (0.20, 0.05, 0.16), metal_uv, cap_color)
    for side in (-1.0, 1.0):
        _box(p, (side * 0.16, 0.12, -0.44), (0.05, 0.05, 0.05), metal_uv, cap_color)
    solid_cylinder(p, (0.0, 0.16, -0.05), 0.070, 5.94, segments=8, taper=0.60,
                   side_uv=metal_uv, cap_uv=metal_uv, color=base)
    p.tube_path([(0.0, 6.09, -0.05), (0.0, 6.25, 0.10), (0.0, 6.34, 0.30), (0.0, 6.36, 0.50)],
                radii=[0.058, 0.054, 0.050, 0.044], segments=6, uv=metal_uv,
                color=_tint(STREET_METAL, 0.42), cap_start=False, cap_end=True)
    # Head: an open-bottomed shell, a lip flange over the pane, and a hood.
    shell_start = len(p.mesh.indices)
    p.cylinder((0.0, 6.08, 0.40), 0.150, 0.22, segments=8, taper=0.82,
               side_uv=metal_uv, cap_uv=metal_uv, color=_tint(STREET_METAL, 0.38), bottom=False)
    orient_outward(p.mesh, shell_start, (0.0, 6.19, 0.40))
    # A square lip ring around the open underside: it frames the pane and
    # keeps the head from reading as a hole in the dark.
    for side in (-1.0, 1.0):
        _box(p, (side * 0.1325, 6.085, 0.40), (0.035, 0.025, 0.23), metal_uv,
             _tint(STREET_METAL, 0.36))
        _box(p, (0.0, 6.085, 0.40 + side * 0.1325), (0.23, 0.025, 0.035), metal_uv,
             _tint(STREET_METAL, 0.36))
    outward_lathe(p, (0.0, 6.30, 0.40),
                  ((0.0, 0.38 * math.sqrt(0.5)), (0.08, 0.14 * math.sqrt(0.5))),
                  segments=4, axis="y", rotation=math.pi * 0.25, uv=metal_uv,
                  color=cap_color, cap_start=True, cap_end=True)

    p.begin_material(pane_slot)
    # The downward pane: quad wound for a -Y outward normal, inside the shell.
    half = 0.115
    p.mesh.quad((0.0 - half, 6.07, 0.40 + half), (0.0 - half, 6.07, 0.40 - half),
                (0.0 + half, 6.07, 0.40 - half), (0.0 + half, 6.07, 0.40 + half),
                uv=pane_uv, color=(255, 255, 255), shade_mult=1.0)
    _fit_to_size(p)

    p.add_note("mount: cast base plate on the floor at y=0, horizontally centred; "
               "the arm and lantern reach +Z, the pane faces -Y")
    p.add_note("post is 6.0 m from the pad top to the arm; the hood shadows upward and the "
               "pane sits in the open underside of the head")
    p.add_note("pane centre is local (0, 6.08, 0.43); suggested level light: shape point, "
               "offset [0, 6.05, 0.43], colour [1.0, 0.84, 0.66], intensity 1.5, range 14.0, "
               "falloff smooth")
    p.add_note("catalogue anchor caveat: the documented offset [0, 6.05, 0.85] lies 0.42 m "
               "beyond this pane and 0.25 m outside the model's 0.6 m +Z half-depth; a 1.2 m "
               "deep, bounding-box-centred model cannot place its pane at z=0.85")


# ---------------------------------------------------------- porch railings
#
# Module 1.8 m, top rail at 1.05 m, painted timber rails with metal balusters.
# The stock is shared by all four pieces: top rail 0.10 x 0.12 m at the
# catalogue height, bottom rail 0.07 x 0.08 m at 0.14 m, balusters 0.03 m
# square. The straight's rail runs edge to edge (x -0.9..0.9), so two modules
# butted at a boundary join with no gap; the corner wraps a right angle around
# a 0.12 m porch post centred on the module corner; the end closes a run with
# a newel whose outer face is flush with the catalogue edge.

RAIL_TOP_Y = 0.95          # top rail underside; its top is the catalogue 1.05 m
RAIL_TOP_H = 0.10
RAIL_BOTTOM_Y = 0.14
RAIL_BOTTOM_H = 0.07
RAIL_DEPTH = 0.12
BALUSTER = 0.030


def _railing_texture(p: PropBuilder, seed: int) -> Texture:
    tex = p.set_texture(128, seed=seed)
    tex.region("wood", (0, 0, 64, 128))
    tex.region("metal", (64, 0, 64, 128))
    _paint_rail_wood(tex, "wood", RAIL_WOOD, seed=seed + 2)
    _paint_rail_metal(tex, "metal", seed=seed + 7)
    return tex


def _rail_run(p: PropBuilder, uv, wood_tint, metal_tint, *, x0: float, x1: float, axis: str = "x",
              top: bool = True, bottom: bool = True, balusters: int = 0) -> None:
    """One rail span along ``axis`` with optional balusters between the rails."""
    length = x1 - x0
    centre = (x0 + x1) * 0.5
    if axis == "x":
        size_top = (length, RAIL_TOP_H, RAIL_DEPTH)
        size_bottom = (length, RAIL_BOTTOM_H, 0.08)
        centre_top = (centre, RAIL_TOP_Y + RAIL_TOP_H * 0.5, 0.0)
        centre_bottom = (centre, RAIL_BOTTOM_Y + RAIL_BOTTOM_H * 0.5, 0.0)
    else:
        size_top = (RAIL_DEPTH, RAIL_TOP_H, length)
        size_bottom = (0.08, RAIL_BOTTOM_H, length)
        centre_top = (0.0, RAIL_TOP_Y + RAIL_TOP_H * 0.5, centre)
        centre_bottom = (0.0, RAIL_BOTTOM_Y + RAIL_BOTTOM_H * 0.5, centre)
    if top:
        _box(p, centre_top, size_top, uv, wood_tint)
    if bottom:
        _box(p, centre_bottom, size_bottom, uv, wood_tint)
    for index in range(balusters):
        t = (index + 1) / (balusters + 1)
        point = x0 + length * t
        height = RAIL_TOP_Y - (RAIL_BOTTOM_Y + RAIL_BOTTOM_H)
        base_y = RAIL_BOTTOM_Y + RAIL_BOTTOM_H
        if axis == "x":
            centre_bal = (point, base_y + height * 0.5, 0.0)
            size_bal = (BALUSTER, height, BALUSTER)
        else:
            centre_bal = (0.0, base_y + height * 0.5, point)
            size_bal = (BALUSTER, height, BALUSTER)
        _box(p, centre_bal, size_bal, uv, metal_tint)


def build_porch_railing_straight(p: PropBuilder) -> None:
    """Straight 1.8 m module: top and bottom rails with nine balusters.

    The rail runs the full catalogue width (x -0.9..0.9), so two modules
    placed edge to edge in a level meet flush and read as one continuous run;
    the rail dies into a ``outdoor:porch_post`` wherever a post is placed on a
    module boundary.
    """
    tex = _railing_texture(p, seed=8001)
    wood_uv = tex.uv("wood", inset=2)
    metal_uv = tex.uv("metal", inset=2)
    wood_slot = p.material("railing_timber")
    metal_slot = p.material("railing_metal")
    wood_tint = _tint(RAIL_WOOD, 0.52)
    metal_tint = _tint(RAIL_METAL, 0.45)

    p.begin_material(wood_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=-0.9, x1=0.9, balusters=0)
    p.begin_material(metal_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=-0.9, x1=0.9, top=False, bottom=False, balusters=9)
    _fit_to_size(p)

    p.add_note("module 1.8 m: top rail at 1.05 m, bottom rail at 0.18 m, nine metal balusters")
    p.add_note("the rail runs the full 1.8 m edge to edge: two modules butted at a module "
               "boundary join with no gap; an outdoor:porch_post on the boundary is the support")
    p.add_note("origin at the floor, centred on the run; non-solid dressing over a prop or wall collider")


def build_porch_railing_corner(p: PropBuilder) -> None:
    """Right-angle corner: two rail stubs wrapping a 0.12 m post.

    The rails cross at the module corner and each reaches the catalogue edge
    (0.15 m from the corner), so both legs meet an adjacent straight or end
    module exactly. A ``outdoor:porch_post`` placed on the same module corner
    is the support; without it the two stubs still read as a corner.
    """
    tex = _railing_texture(p, seed=8101)
    wood_uv = tex.uv("wood", inset=2)
    metal_uv = tex.uv("metal", inset=2)
    wood_slot = p.material("railing_timber")
    metal_slot = p.material("railing_metal")
    wood_tint = _tint(RAIL_WOOD, 0.52)
    metal_tint = _tint(RAIL_METAL, 0.45)

    p.begin_material(wood_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=0.0, x1=0.15, axis="x", balusters=0)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=0.0, x1=0.15, axis="z", balusters=0)
    p.begin_material(metal_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=0.0, x1=0.15, axis="x", top=False, bottom=False,
              balusters=1)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=0.0, x1=0.15, axis="z", top=False, bottom=False,
              balusters=1)
    _fit_to_size(p)

    p.add_note("right-angle corner: rails cross on the module corner and reach the 0.30 m "
               "catalogue edge on both legs (+X and +Z at rotation 0; rotate 90/180/270 for "
               "the other corners)")
    p.add_note("pair it with outdoor:porch_post on the same module corner: the corner wraps "
               "the 0.12 m post and the post is the support")
    p.add_note("the two rail ends meet an adjacent straight or end module at the corner's "
               "0.15 m edges, so a run has no gap")


def build_porch_railing_end(p: PropBuilder) -> None:
    """Run terminal: a short rail span and a newel closing the run.

    The newel (0.12 m square, capped at the catalogue height) sits at the +X
    edge; the rail reaches the -X edge. Place the end piece so its -X edge
    meets the last straight module's rail end (0.15 m from the run's terminal
    module boundary).
    """
    tex = _railing_texture(p, seed=8201)
    wood_uv = tex.uv("wood", inset=2)
    metal_uv = tex.uv("metal", inset=2)
    wood_slot = p.material("railing_timber")
    metal_slot = p.material("railing_metal")
    wood_tint = _tint(RAIL_WOOD, 0.52)
    metal_tint = _tint(RAIL_METAL, 0.45)

    p.begin_material(wood_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=-0.15, x1=0.05, balusters=0)
    # Newel: a 0.12 m square post at the outer edge, capped at the 1.05 m top.
    _box(p, (0.09, 0.475, 0.0), (0.118, 0.95, 0.118), wood_uv, wood_tint)
    _box(p, (0.09, 1.00, 0.0), (0.12, 0.10, 0.12), wood_uv, _tint(RAIL_WOOD, 0.46))
    _box(p, (0.09, 0.055, 0.0), (0.12, 0.11, 0.12), wood_uv, _tint(RAIL_WOOD, 0.94))
    p.begin_material(metal_slot)
    _rail_run(p, wood_uv, wood_tint, metal_tint, x0=-0.15, x1=-0.05, top=False, bottom=False, balusters=1)
    _fit_to_size(p)

    p.add_note("run terminal: the rail reaches the -X edge and a capped 0.12 m newel closes "
               "the +X end; place it so its -X edge meets the last straight module's rail end")
    p.add_note("the newel's outer face is flush with the 0.30 m catalogue edge, so an "
               "adjacent module meets it with no gap")
    p.add_note("matched top and bottom rails with the straight module; one timber newel, one "
               "metal baluster")


def build_porch_post(p: PropBuilder) -> None:
    """Square porch post: support and porch-roof seat at 1.05 m.

    Base on the floor at ``y = 0``; the flat cap seat tops out at the catalogue
    height 1.05 m. The fences' ``outdoor:fence_post`` is the same idea in the
    lamp family; this is the railing/support piece.
    """
    size = p.size  # [0.12, 1.05, 0.12]
    tex = p.set_texture(128, seed=8301)
    tex.region("post", (0, 0, 128, 64))
    tex.region("cap", (0, 64, 128, 64))
    _paint_post_timber(tex, "post", RAIL_WOOD, seed=8303)
    tex.fill("cap", palette.shade(RAIL_WOOD, 0.78), jitter=5, seed=8309)
    tex.grain("cap", palette.shade(RAIL_WOOD, 0.5), seed=8311, density=0.4, alpha=55)
    post_uv = tex.uv("post", inset=2)
    cap_uv = tex.uv("cap", inset=2)
    slot = p.material("porch_post")
    p.begin_material(slot)
    half_side = size[0] * 0.5
    solid_cylinder(p, (0.0, 0.0, 0.0), half_side * math.sqrt(2.0), 0.985,
                   segments=4, rotation=math.pi * 0.25, taper=0.86,
                   side_uv=post_uv, cap_uv=cap_uv, color=_tint(RAIL_WOOD, 0.44))
    _box(p, (0.0, 1.02, 0.0), (size[0], 0.06, size[2]),
         {"+y": cap_uv, "-y": post_uv, "+x": post_uv, "-x": post_uv, "+z": post_uv, "-z": post_uv},
         _tint(RAIL_WOOD, 0.50))
    _fit_to_size(p)

    p.add_note("square porch post with a flat cap seat at y = 1.05; the railing's support and "
               "the seat for a porch roof")
    p.add_note("shaft tapers 0.12 m to 0.103 m square; one opaque timber material")
    p.add_note("place it on a module boundary or module corner behind the railing modules")


# ------------------------------------------------------------ house facades
#
# Shared construction, identical across the five families:
#
# * a 3.0 m module, 2.7 m high; panels 0.24 m deep, doorway panels 0.40 m;
# * the cladding core is inset so two corner boards own the panel's x extents,
#   and a thin cap board owns the top (y 2.66..2.70);
# * the doorway cuts a real 1.10 x 2.15 m hole through the depth with jambs,
#   head and a threshold board; eave lamp mounts at local (+/-1.15, 2.55, 0.20);
# * the gable is a triangular 3.0 x 1.02 x 0.24 panel for the 1.75/2.6 pitch;
# * the roof slope rises towards local -Z with the origin at the eave underside
#   centre; the ridge cap apex edge sits at local y = 0.22;
# * the porch deck extends +Z from the local wall plane at z = 0 (place its
#   origin 0.75 m out from the wall face) and the porch post's top is 2.30 m.
#
# The type variation lives in the cladding painter, the window pattern, the
# trim profile, the gable bargeboard and the porch treatment.

PANEL_WIDTH = 3.0
PANEL_HEIGHT = 2.7
PANEL_DEPTH = 0.24
DOORWAY_DEPTH = 0.40
CORE_HALF = 1.285
CORE_HEIGHT = 2.664
GABLE_APEX = 1.02
ROOF_WIDTH = 3.4
ROOF_HEIGHT = 1.75
ROOF_DEPTH = 2.6


def _panel_uvs(siding_uv, trim_uv, jamb_uv):
    return {
        "+z": siding_uv, "-z": siding_uv, "+x": jamb_uv, "-x": siding_uv,
        "+y": trim_uv, "-y": trim_uv,
    }


def _clad_uv(tex: Texture, x: float, y: float) -> tuple[float, float]:
    """A point on the 3.0 x 2.7 m module mapped into the siding region.

    The panel is built from several boxes (columns around an opening, a sill
    band, a head band). Painting the whole region onto each box would restart
    the cladding at every box edge, so every face samples the sub-rectangle its
    own metres occupy: the boards run unbroken across the whole panel.
    """
    rx, ry, width, height = tex.cell("siding")
    fx = (x + PANEL_WIDTH * 0.5) / PANEL_WIDTH
    fy = (PANEL_HEIGHT - y) / PANEL_HEIGHT
    return ((rx + fx * width) / tex.width, (ry + fy * height) / tex.height)


def _clad_face_uvs(tex: Texture, x0: float, x1: float, y0: float, y1: float):
    """Corner UVs (bottom-left, bottom-right, top-right, top-left) for a box."""
    return [
        _clad_uv(tex, x0, y0), _clad_uv(tex, x1, y0),
        _clad_uv(tex, x1, y1), _clad_uv(tex, x0, y1),
    ]


def _clad_faces(tex: Texture, x0: float, x1: float, y0: float, y1: float, trim_uv, jamb_uv,
                reveal=("+x",)):
    """Per-face UVs for a cladding core box spanning its own module rectangle.

    ``reveal`` names the box faces that look into an opening: those get the
    jamb (end-grain) texture, everything else gets the continuous cladding.
    """
    clad = _clad_face_uvs(tex, x0, x1, y0, y1)
    faces = {
        "+z": clad, "-z": clad, "+x": clad, "-x": clad,
        "+y": trim_uv, "-y": trim_uv,
    }
    for key in reveal:
        faces[key] = jamb_uv
    return faces


def _corner_boards(p: PropBuilder, depth: float, top: float, uv, color) -> None:
    """Two corner boards owning the panel's width extents (x = +/-1.5)."""
    for side in (-1.0, 1.0):
        _box(p, (side * 1.3875, top * 0.5, 0.0), (0.225, top, depth), uv, color)


def _panel_cap(p: PropBuilder, depth: float, uv, color) -> None:
    """Thin cap board over the panel top (y 2.66..2.70), full width/depth."""
    _box(p, (0.0, 2.68, 0.0), (3.0, 0.04, depth), uv, color)


def _build_wall_solid(p: PropBuilder, family: dict, tex: Texture) -> None:
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    jamb_uv = tex.uv("jamb", inset=2)
    clad = _tint(family["siding"], 0.55)
    trim = _tint(family["trim"], 0.50)
    _box(p, (0.0, CORE_HEIGHT * 0.5, 0.0), (2.57, CORE_HEIGHT, 0.22),
         _clad_faces(tex, -CORE_HALF, CORE_HALF, 0.0, CORE_HEIGHT, trim_uv, jamb_uv), clad)
    _corner_boards(p, PANEL_DEPTH, 2.66, trim_uv, trim)
    _panel_cap(p, PANEL_DEPTH, trim_uv, trim)
    style = family["cladding"]
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: {style} cladding, painted 0.35 m plinth band, "
               "corner boards and a cap board; one opaque material, 0.24 m panel depth")
    p.add_note("use a level size of [3.0, 2.7, 0.24]; +Z is the front")


def _window_dress(p: PropBuilder, family: dict, rect, uv_glass, uv_trim, trim, cols: int, rows: int) -> None:
    """One framed window: recessed glazing, trim frame, mullions, sill and head."""
    cx, cy, w, h = rect
    glass = (238, 242, 248)
    solid_box(p, (cx, cy, 0.02), (w + 0.04, h + 0.04, 0.03),
              uv={"+z": uv_glass, "-z": uv_glass, "+x": uv_trim, "-x": uv_trim,
                  "+y": uv_trim, "-y": uv_trim}, color=glass)
    dress = family["window_dress"]
    jamb = 0.075 if dress != "narrow" else 0.055
    # Every dress board lies inside the panel's 0.24 m depth: z = 0.11 with a
    # 0.02 depth keeps the front face at exactly +0.12.
    for side in (-1.0, 1.0):
        _box(p, (cx + side * (w * 0.5 + jamb * 0.5), cy, 0.110), (jamb, h + jamb * 2.0, 0.02),
             uv_trim, trim)
    for side in (-1.0, 1.0):
        _box(p, (cx, cy + side * (h * 0.5 + jamb * 0.5), 0.110), (w + jamb * 2.0, jamb, 0.02),
             uv_trim, trim)
    for index in range(1, cols):
        _box(p, (cx - w * 0.5 + w * index / cols, cy, 0.095), (0.05, h, 0.02), uv_trim, trim)
    for index in range(1, rows):
        _box(p, (cx, cy - h * 0.5 + h * index / rows, 0.095), (w, 0.05, 0.02), uv_trim, trim)
    sill_w = w + 0.34 if dress in ("apron", "bracketed") else w + 0.26
    _box(p, (cx, cy - h * 0.5 - jamb - 0.035, 0.110), (sill_w, 0.07, 0.02), uv_trim, trim)
    head_w = w + 0.30 if dress != "narrow" else w + 0.18
    _box(p, (cx, cy + h * 0.5 + jamb + 0.030, 0.110), (head_w, 0.06, 0.02), uv_trim, trim)
    if dress == "pediment":
        _box(p, (cx, cy + h * 0.5 + jamb + 0.11, 0.110), (w + 0.46, 0.09, 0.02), uv_trim,
             palette.shade(trim, 0.92))
    elif dress == "apron":
        _box(p, (cx, cy - h * 0.5 - jamb - 0.10, 0.108), (w - 0.06, 0.10, 0.02), uv_trim,
             palette.shade(trim, 1.04))
    elif dress == "bracketed":
        for side in (-1.0, 1.0):
            _box(p, (cx + side * (w * 0.5 + 0.16), cy + h * 0.5 + 0.06, 0.110), (0.09, 0.16, 0.02),
                 uv_trim, trim)


def _build_wall_window(p: PropBuilder, family: dict, tex: Texture) -> None:
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    glass_uv = tex.uv("glass", inset=2)
    jamb_uv = tex.uv("jamb", inset=2)
    clad = _tint(family["siding"], 0.55)
    trim = _tint(family["trim"], 0.50)
    windows = family["windows"]
    xs = [-CORE_HALF]
    for cx, _, w, _, _, _ in windows:
        xs.append(cx - w * 0.5)
        xs.append(cx + w * 0.5)
    xs.append(CORE_HALF)
    for index in range(0, len(xs), 2):
        a, b = xs[index], xs[index + 1]
        if b - a > 1e-6:
            reveal = []
            if index > 0:
                reveal.append("-x")
            if index + 1 < len(xs) - 1:
                reveal.append("+x")
            _box(p, ((a + b) * 0.5, CORE_HEIGHT * 0.5, 0.0), (b - a, CORE_HEIGHT, 0.22),
                 _clad_faces(tex, a, b, 0.0, CORE_HEIGHT, trim_uv, jamb_uv, tuple(reveal)), clad)
    # The sill and head bands belong to each opening's own x range, so they
    # never share a plane with the columns between two windows.
    for cx, cy, w, h, _, _ in windows:
        top = cy + h * 0.5
        bottom = cy - h * 0.5
        if bottom > 0.01:
            _box(p, (cx, bottom * 0.5, 0.0), (w, bottom, 0.22),
                 _clad_faces(tex, cx - w * 0.5, cx + w * 0.5, 0.0, bottom, trim_uv, jamb_uv, ("+y",)), clad)
        if top < CORE_HEIGHT - 0.01:
            _box(p, (cx, (top + CORE_HEIGHT) * 0.5, 0.0), (w, CORE_HEIGHT - top, 0.22),
                 _clad_faces(tex, cx - w * 0.5, cx + w * 0.5, top, CORE_HEIGHT, trim_uv, jamb_uv, ("-y",)), clad)
    for cx, cy, w, h, cols, rows in windows:
        _window_dress(p, family, (cx, cy, w, h), glass_uv, trim_uv, trim, cols, rows)
    _corner_boards(p, PANEL_DEPTH, 2.66, trim_uv, trim)
    _panel_cap(p, PANEL_DEPTH, trim_uv, trim)
    _fit_to_size(p)
    pattern = ", ".join(f"{cols}x{rows} at ({cx:+.2f}, {cy:.2f})" for cx, cy, _, _, cols, rows in windows)
    p.add_note(f"{family['name'].lower()}: framed window pattern {pattern}; opaque dark "
               "glazing set 7 cm behind the front face with a real reveal")
    p.add_note("use a level size of [3.0, 2.7, 0.24]; the windows are decorative and the "
               "panel stays solid unless the level also cuts a real wall opening")


def _build_wall_doorway(p: PropBuilder, family: dict, tex: Texture) -> None:
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    jamb_uv = tex.uv("jamb", inset=2)
    clad = _tint(family["siding"], 0.55)
    trim = _tint(family["trim"], 0.50)
    # Core in three boxes around the 1.10 x 2.15 m opening through 0.40 m.
    for side in (-1.0, 1.0):
        x0 = 0.55 if side > 0 else -CORE_HALF
        x1 = CORE_HALF if side > 0 else -0.55
        reveal = ("+x",) if side < 0 else ("-x",)
        _box(p, ((x0 + x1) * 0.5, CORE_HEIGHT * 0.5, 0.0), (x1 - x0, CORE_HEIGHT, DOORWAY_DEPTH),
             _clad_faces(tex, x0, x1, 0.0, CORE_HEIGHT, trim_uv, jamb_uv, reveal), clad)
    head_faces = _clad_faces(tex, -0.555, 0.555, 2.15, CORE_HEIGHT, trim_uv, jamb_uv)
    head_faces["-y"] = jamb_uv
    _box(p, (0.0, 2.407, 0.0), (1.11, 0.514, DOORWAY_DEPTH), head_faces, clad)
    _box(p, (0.0, 0.015, 0.0), (1.12, 0.03, 0.38),
         {"+y": trim_uv, "-y": trim_uv, "+z": trim_uv, "-z": trim_uv,
          "+x": jamb_uv, "-x": jamb_uv}, _tint(palette.hex_to_rgb(palette.WOOD_MID), 0.50))
    # Door surround: jambs and head proud of both faces, with the type's hood.
    for side in (-1.0, 1.0):
        _box(p, (side * 0.60, 1.075, 0.0), (0.10, 2.15, 0.38), trim_uv, trim)
    _box(p, (0.0, 2.20, 0.0), (1.30, 0.10, 0.38), trim_uv, trim)
    if family["window_dress"] == "pediment":
        _box(p, (0.0, 2.34, 0.0), (1.46, 0.12, 0.36), trim_uv,
             palette.shade(trim, 0.93))
    elif family["window_dress"] == "bracketed":
        for side in (-1.0, 1.0):
            _box(p, (side * 0.60, 2.33, 0.0), (0.12, 0.16, 0.38), trim_uv,
                 palette.shade(trim, 1.02))
    _corner_boards(p, DOORWAY_DEPTH, 2.66, trim_uv, trim)
    _panel_cap(p, DOORWAY_DEPTH, trim_uv, trim)
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: real 1.10 x 2.15 m doorway through 0.40 m with "
               "jambs, head and a 0.03 m threshold board")
    p.add_note("non-solid; the level authors the real wall and door behind it (offset the "
               "panel 0.20 m outward from the wall face)")
    p.add_note("eave lamp mounts: local [+/-1.15, 2.55, 0.20], +Z away from the wall "
               "(outdoor:lamp_wall's plate top meets the mount, so place it 0.52 m lower)")


def _build_gable(p: PropBuilder, family: dict, tex: Texture) -> None:
    siding_uv = tex.uv("siding", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    clad = _tint(family["siding"], 0.55)
    trim = _tint(family["trim"], 0.50)
    # Cladding face: the triangle maps the top 1.02 m of the siding region, so
    # its board spacing matches the 2.7 m wall module below it.
    u0, v0, u1, v1 = tex.sub("siding", 0.0, 0.0, 1.0, GABLE_APEX / PANEL_HEIGHT)
    mid_u = (u0 + u1) * 0.5
    start = len(p.mesh.indices)
    p.mesh.triangle((-1.5, 0.0, 0.12), (1.5, 0.0, 0.12), (0.0, GABLE_APEX, 0.12),
                    uvs=[(u0, v1), (u1, v1), (mid_u, v0)], color=clad, shade_mult=0.90)
    p.mesh.triangle((1.5, 0.0, -0.12), (-1.5, 0.0, -0.12), (0.0, GABLE_APEX, -0.12),
                    uvs=[(u1, v1), (u0, v1), (mid_u, v0)], color=clad, shade_mult=0.80)
    p.mesh.quad((-1.5, 0.0, 0.12), (0.0, GABLE_APEX, 0.12), (0.0, GABLE_APEX, -0.12), (-1.5, 0.0, -0.12),
                uv=trim_uv, color=trim, shade_mult=0.86)
    p.mesh.quad((0.0, GABLE_APEX, 0.12), (1.5, 0.0, 0.12), (1.5, 0.0, -0.12), (0.0, GABLE_APEX, -0.12),
                uv=trim_uv, color=trim, shade_mult=0.86)
    p.mesh.quad((-1.5, 0.0, -0.12), (1.5, 0.0, -0.12), (1.5, 0.0, 0.12), (-1.5, 0.0, 0.12),
                uv=trim_uv, color=trim, shade_mult=0.62)
    orient_outward(p.mesh, start, (0.0, 0.36, 0.0))
    # Bargeboards: raked boards riding just inside the two sloped edges.
    style = family["barge"]
    rake = math.degrees(math.atan2(GABLE_APEX, 1.5))
    length = math.hypot(1.5, GABLE_APEX)
    board = 0.16 if style in ("stepped", "heavy") else 0.12
    thickness = 0.07 if style == "heavy" else 0.05
    proud = 0.125 if style != "heavy" else 0.135
    for side in (-1.0, 1.0):
        angle = rake * (-side)
        centre = (side * 0.75, GABLE_APEX * 0.5, proud)
        _rotate_last_box(p, centre, (length, board, thickness), trim_uv, trim, angle)
        if style in ("stepped", "heavy"):
            inner = (side * 0.70, GABLE_APEX * 0.44, proud - 0.05)
            _rotate_last_box(p, inner, (length * 0.92, board * 0.72, thickness * 0.8), trim_uv,
                             palette.shade(trim, 0.86), angle)
    if family["cladding"] == "battens":
        _box(p, (0.0, 0.30, 0.125), (0.54, 0.52, 0.02), trim_uv, palette.shade(trim, 0.90))
    if style == "plain" and family["ridge_style"] == "wide":
        # A tiny attic vent keeps the white-shiplap gable from reading blank.
        _box(p, (0.0, 0.36, 0.126), (0.30, 0.30, 0.02), trim_uv, palette.shade(trim, 0.94))
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: triangular gable panel for a 3 m module at the "
               f"kit's 1.75/2.6 roof pitch, with the type's {style} bargeboard")
    p.add_note("base at y = 0: seat it on a 2.7 m wall module; use a level size of "
               "[3.0, 1.02, 0.24]")


def _rotate_last_box(p: PropBuilder, centre, size, uv, color, degrees: float) -> None:
    """Adds a box and leans exactly its own vertices about ``centre`` on Z.

    The gable's bargeboards are straight boards raked onto the roof pitch: the
    board is built level and then rotated about its own centre, which keeps the
    primitive (and its UVs) identical to every other trim board.
    """
    start = len(p.mesh.positions)
    _box(p, centre, size, uv, color)
    radians = math.radians(degrees)
    cos, sin = math.cos(radians), math.sin(radians)
    for index in range(start, len(p.mesh.positions)):
        x, y, z = p.mesh.positions[index]
        dx, dy = x - centre[0], y - centre[1]
        p.mesh.positions[index] = (
            round(centre[0] + dx * cos - dy * sin, 6),
            round(centre[1] + dx * sin + dy * cos, 6),
            z,
        )


def _build_roof_slope(p: PropBuilder, family: dict, tex: Texture) -> None:
    shingle_uv = tex.uv("shingle", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    shingle = _tint(family["shingle"], 0.32)
    trim = _tint(family["roof_trim"], 0.50)
    width, height, depth = p.size  # [3.4, 1.75, 2.6]
    half_x = width * 0.5
    eave_z, ridge_z = depth * 0.5, -depth * 0.5
    eave_top, ridge_top = 0.10, height
    slab_eave_z = eave_z - 0.01

    def quad(p0, p1, p2, p3, uv, color, shade):
        p.mesh.quad(p0, p1, p2, p3, uv=uv, color=color, shade_mult=shade)

    # Shingle field, eave (+Z, low) to ridge (-Z, high).
    quad((-half_x, eave_top, slab_eave_z), (half_x, eave_top, slab_eave_z),
         (half_x, ridge_top, ridge_z), (-half_x, ridge_top, ridge_z),
         shingle_uv, shingle, 1.0)
    # Underside and the two edge strips.
    quad((-half_x, eave_top - 0.08, slab_eave_z), (-half_x, ridge_top - 0.08, ridge_z),
         (half_x, ridge_top - 0.08, ridge_z), (half_x, eave_top - 0.08, slab_eave_z),
         trim_uv, trim, 0.62)
    quad((-half_x, eave_top - 0.08, slab_eave_z), (half_x, eave_top - 0.08, slab_eave_z),
         (half_x, eave_top, slab_eave_z), (-half_x, eave_top, slab_eave_z),
         trim_uv, trim, 0.80)
    quad((half_x, ridge_top - 0.08, ridge_z), (-half_x, ridge_top - 0.08, ridge_z),
         (-half_x, ridge_top, ridge_z), (half_x, ridge_top, ridge_z),
         trim_uv, trim, 0.80)
    quad((half_x, eave_top - 0.08, slab_eave_z), (half_x, ridge_top - 0.08, ridge_z),
         (half_x, ridge_top, ridge_z), (half_x, eave_top, slab_eave_z),
         trim_uv, trim, 0.82)
    quad((-half_x, eave_top - 0.08, slab_eave_z), (-half_x, eave_top, slab_eave_z),
         (-half_x, ridge_top, ridge_z), (-half_x, ridge_top - 0.08, ridge_z),
         trim_uv, trim, 0.82)
    # Fascia and soffit: the type's eave width, plus optional eave brackets.
    fascia_h = 0.16 if family["barge"] != "heavy" else 0.19
    fascia_centre = fascia_h * 0.5
    _box(p, (0.0, fascia_centre, eave_z - 0.03), (width, fascia_h, 0.06),
         {key: trim_uv for key in FACE_KEYS}, trim)
    soffit_w = 0.28 if family["deck_style"] not in ("deep", "steps2") else 0.34
    _box(p, (0.0, 0.0175, 1.10), (width, 0.035, soffit_w),
         {key: trim_uv for key in FACE_KEYS}, trim)
    for index in range(family["brackets"]):
        t = (index + 1) / (family["brackets"] + 1)
        _box(p, (-half_x + width * t, fascia_centre + 0.10, eave_z - 0.07), (0.10, 0.16, 0.10),
             {key: trim_uv for key in FACE_KEYS}, palette.shade(trim, 0.88))
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: shingle slope rising towards -Z with a "
               f"{fascia_h:.2f} m fascia and a {soffit_w:.2f} m soffit at the eave")
    p.add_note("origin at the eave's underside centre; two identical slopes mirrored 180 "
               "degrees meet at an outdoor:house_0N_roof_ridge")


def _build_roof_ridge(p: PropBuilder, family: dict, tex: Texture) -> None:
    shingle_uv = tex.uv("shingle", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    shingle = _tint(family["shingle"], 0.32)
    trim = _tint(family["roof_trim"], 0.50)
    width, height, depth = p.size  # [3.4, 0.22, 0.6]
    half_x, half_z = width * 0.5, depth * 0.5
    p.mesh.quad((-half_x, 0.0, half_z), (half_x, 0.0, half_z),
                (half_x, height, 0.0), (-half_x, height, 0.0),
                uv=shingle_uv, color=shingle, shade_mult=1.0)
    p.mesh.quad((-half_x, height, 0.0), (half_x, height, 0.0),
                (half_x, 0.0, -half_z), (-half_x, 0.0, -half_z),
                uv=shingle_uv, color=shingle, shade_mult=0.94)
    p.mesh.triangle((-half_x, height, 0.0), (-half_x, 0.0, -half_z), (-half_x, 0.0, half_z),
                    uvs=[((shingle_uv[0] + shingle_uv[2]) * 0.5, (shingle_uv[1] + shingle_uv[3]) * 0.5),
                         (shingle_uv[0], shingle_uv[3]), (shingle_uv[2], shingle_uv[3])],
                    color=shingle, shade_mult=0.78)
    p.mesh.triangle((half_x, height, 0.0), (half_x, 0.0, half_z), (half_x, 0.0, -half_z),
                    uvs=[((shingle_uv[0] + shingle_uv[2]) * 0.5, (shingle_uv[1] + shingle_uv[3]) * 0.5),
                         (shingle_uv[2], shingle_uv[3]), (shingle_uv[0], shingle_uv[3])],
                    color=shingle, shade_mult=0.78)
    p.mesh.quad((-half_x, 0.0, half_z), (-half_x, 0.0, -half_z),
                (half_x, 0.0, -half_z), (half_x, 0.0, half_z),
                uv=trim_uv, color=trim, shade_mult=0.62)
    style = family["ridge_style"]
    if style == "board":
        # The ridge board rides under the apex so the documented apex edge
        # (local y = 0.22) still owns the top of the bounding box.
        _box(p, (0.0, 0.20, 0.0), (width, 0.04, 0.16), trim_uv, palette.shade(trim, 0.92))
    elif style == "wide":
        _box(p, (0.0, 0.20, 0.0), (width, 0.04, 0.24), trim_uv, palette.shade(trim, 0.98))
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: ridge cap with the apex edge at local y = 0.22, "
               f"underside at y = 0 ({style} ridge board)")
    p.add_note("place it at the wall top + 1.75 - 0.22 between two mirrored slopes")


def _build_corner_trim(p: PropBuilder, family: dict, tex: Texture) -> None:
    trim_uv = tex.uv("trim", inset=2)
    edge_uv = tex.uv("jamb", inset=2) if "jamb" in tex.regions else trim_uv
    trim = _tint(family["trim"], 0.50)
    size = p.size  # [0.18, 2.7, 0.18]
    style = family["corner_style"]
    leg = 0.18
    if style == "wide":
        leg = 0.20
    # Leg A covers the -Z side, leg B returns along -X; they meet inside so no
    # visible faces are coplanar.
    _box(p, (0.0, size[1] * 0.5, -leg * 0.5 + 0.03), (leg, size[1], leg * 0.5 - 0.03),
         {"+z": trim_uv, "-z": trim_uv, "+x": edge_uv, "-x": trim_uv, "+y": edge_uv, "-y": edge_uv},
         trim)
    _box(p, (-leg * 0.5 + 0.03, size[1] * 0.5, leg * 0.25), (leg * 0.5 - 0.03, size[1], leg * 0.5 + 0.03),
         {"+z": trim_uv, "-z": trim_uv, "+x": trim_uv, "-x": trim_uv, "+y": edge_uv, "-y": edge_uv},
         trim)
    if style == "bead":
        _box(p, (leg * 0.5 - 0.02, size[1] * 0.5, -0.005), (0.035, size[1], 0.035), trim_uv,
             palette.shade(trim, 0.92))
    elif style == "batten":
        _box(p, (-leg * 0.5 + 0.02, size[1] * 0.5, 0.005), (0.05, size[1], 0.05), trim_uv,
             palette.shade(trim, 0.88))
    elif style == "mitre":
        _box(p, (0.0, size[1] * 0.5, -0.005), (0.06, size[1], 0.06), trim_uv,
             palette.shade(trim, 1.06), shade=False)
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: {style} L-section corner board covering a panel "
               "junction; base on the floor at y=0")
    p.add_note("outer faces look towards -X and -Z: rotate 0/90/180/270 to cover any corner")


def _build_porch_deck(p: PropBuilder, family: dict, tex: Texture) -> None:
    deck_uv = tex.uv("deck", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    deck = _tint(family["deck"], 0.55)
    trim = _tint(family["trim"], 0.50)
    style = family["deck_style"]
    top = 0.24
    lower = 0.12
    # Main slab: z -0.75..front, top at 0.24; a lower tread owns the outer edge
    # so the step edge is real geometry, not a painted line.
    front = 0.75
    if style == "stoop":
        tread = 0.33
    elif style == "small":
        tread = 0.26
    else:
        tread = 0.40
    main_front = front - tread
    _box(p, (0.0, top * 0.5, (-front + main_front) * 0.5), (3.0, top, main_front + front),
         {"+y": deck_uv, "+z": trim_uv, "-z": trim_uv, "+x": deck_uv, "-x": deck_uv,
          "-y": trim_uv}, deck)
    if style == "deep":
        # Two treads for the deepest porch.
        _box(p, (0.0, lower * 0.5, main_front + tread * 0.25), (3.0, lower, tread * 0.5),
             {"+y": deck_uv, "+z": trim_uv, "-z": trim_uv, "+x": deck_uv, "-x": deck_uv,
              "-y": trim_uv}, deck)
        _box(p, (0.0, 0.06, main_front + tread * 0.75), (3.0, 0.12, tread * 0.5),
             {"+y": deck_uv, "+z": trim_uv, "-z": trim_uv, "+x": deck_uv, "-x": deck_uv,
              "-y": trim_uv}, palette.shade(deck, 0.94))
    else:
        _box(p, (0.0, lower * 0.5, main_front + tread * 0.5), (3.0, lower, tread),
             {"+y": deck_uv, "+z": trim_uv, "-z": trim_uv, "+x": deck_uv, "-x": deck_uv,
              "-y": trim_uv}, deck)
    # Nosing over the main slab's front edge, and a skirt board around the base.
    _box(p, (0.0, top - 0.015, main_front + 0.02), (3.0, 0.05, 0.06), trim_uv,
         palette.shade(trim, 1.04))
    _box(p, (0.0, 0.05, -front + 0.025), (3.0, 0.10, 0.05), trim_uv, palette.shade(trim, 0.94))
    for side in (-1.0, 1.0):
        _box(p, (side * 1.475, 0.05, 0.0), (0.05, 0.10, front * 2.0), trim_uv,
             palette.shade(trim, 0.90))
    if style in ("steps2", "deep"):
        for side in (-1.0, 1.0):
            _box(p, (side * 1.35, 0.10, -front + 0.20), (0.20, 0.20, 0.20), trim_uv,
                 palette.shade(trim, 0.86))
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: {style} porch deck for one 3 m module, extending "
               "+Z from the local wall plane at z = 0, base at y = 0")
    p.add_note("place its origin 0.75 m out from the wall face: the deck's back edge then "
               "sits against the wall base and its front edge is 1.5 m out")
    p.add_note("the front edge is a real step (a lower tread and a nosing); "
               "outdoor:house_0N_porch_post seats against it")


def _build_porch_post(p: PropBuilder, family: dict, tex: Texture) -> None:
    post_uv = tex.uv("post", inset=2)
    trim_uv = tex.uv("trim", inset=2)
    post_tint = _tint(family["post"], 0.50)
    trim = _tint(family["trim"], 0.50)
    style = family["post_style"]
    # Everything stays inside the 0.14 m square box: the post is the support.
    if style == "round":
        solid_cylinder(p, (0.0, 0.0, 0.0), 0.070, 2.14, segments=8, taper=0.92,
                       side_uv=post_uv, cap_uv=trim_uv, color=post_tint)
        _box(p, (0.0, 0.20, 0.0), (0.14, 0.06, 0.14), trim_uv, palette.shade(trim, 0.98))
    elif style == "chamfer":
        solid_cylinder(p, (0.0, 0.0, 0.0), 0.099 * math.sqrt(0.5) * 2.0, 2.14, segments=4,
                       rotation=math.pi * 0.25, taper=0.90, side_uv=post_uv, cap_uv=post_uv,
                       color=post_tint)
        _box(p, (0.0, 2.20, 0.0), (0.14, 0.06, 0.14), trim_uv, palette.shade(trim, 1.04))
        _box(p, (0.0, 0.10, 0.0), (0.14, 0.06, 0.14), trim_uv, palette.shade(trim, 0.92))
    elif style == "two_stage":
        solid_cylinder(p, (0.0, 0.0, 0.0), 0.099 * math.sqrt(0.5) * 2.0 * 0.5, 0.60, segments=4,
                       rotation=math.pi * 0.25, taper=0.94, side_uv=post_uv, cap_uv=post_uv,
                       color=palette.shade(post_tint, 0.94))
        solid_cylinder(p, (0.0, 0.60, 0.0), 0.099 * math.sqrt(0.5) * 2.0 * 0.47, 1.70, segments=4,
                       rotation=math.pi * 0.25, taper=0.92, side_uv=post_uv, cap_uv=post_uv,
                       color=post_tint)
        _box(p, (0.0, 0.60, 0.0), (0.14, 0.05, 0.14), trim_uv, palette.shade(trim, 0.98))
        _box(p, (0.0, 2.24, 0.0), (0.14, 0.06, 0.14), trim_uv, palette.shade(trim, 1.02))
    else:  # square / capital
        solid_cylinder(p, (0.0, 0.0, 0.0), 0.099 * math.sqrt(0.5) * 2.0 * 0.5, 2.14, segments=4,
                       rotation=math.pi * 0.25, taper=0.90, side_uv=post_uv, cap_uv=post_uv,
                       color=post_tint)
        _box(p, (0.0, 2.24, 0.0), (0.14, 0.06, 0.14), trim_uv, palette.shade(trim, 1.02))
        if style == "capital":
            _box(p, (0.0, 2.16, 0.0), (0.14, 0.10, 0.14), trim_uv, palette.shade(trim, 0.92))
            _box(p, (0.0, 0.10, 0.0), (0.14, 0.08, 0.14), trim_uv, palette.shade(trim, 0.94))
    _fit_to_size(p)
    p.add_note(f"{family['name'].lower()}: {style} porch post, base at y = 0, flat top; the "
               "porch deck's front edge seats against it")
    p.add_note("use a level size of [0.14, 2.3, 0.14]; the flat top seats a porch roof or beam")


_HOUSE_BUILDERS = {
    "wall_solid": _build_wall_solid,
    "wall_window": _build_wall_window,
    "wall_doorway": _build_wall_doorway,
    "gable": _build_gable,
    "roof_slope": _build_roof_slope,
    "roof_ridge": _build_roof_ridge,
    "corner_trim": _build_corner_trim,
    "porch_deck": _build_porch_deck,
    "porch_post": _build_porch_post,
}

#: Regions each piece paints; small pieces use a 128 px sheet.
_PIECE_REGIONS = {
    "wall_solid": ("siding", "trim", "jamb"),
    "wall_window": ("siding", "trim", "glass", "jamb"),
    "wall_doorway": ("siding", "trim", "jamb"),
    "gable": ("siding", "trim"),
    "roof_slope": ("shingle", "trim"),
    "roof_ridge": ("shingle", "trim"),
    "corner_trim": ("trim", "jamb"),
    "porch_deck": ("deck", "trim"),
    "porch_post": ("post", "trim"),
}
_PIECE_TEXTURE = {
    "wall_solid": 256, "wall_window": 256, "wall_doorway": 256, "gable": 256,
    "roof_slope": 256, "roof_ridge": 128, "corner_trim": 128, "porch_deck": 256,
    "porch_post": 128,
}


def _make_house_builder(piece: str, family: dict):
    build = _HOUSE_BUILDERS[piece]

    def builder(p: PropBuilder) -> None:
        tex = _family_atlas(p, family, _PIECE_TEXTURE[piece], _PIECE_REGIONS[piece])
        build(p, family, tex)

    builder.__name__ = f"build_house_{family['name'].split()[0].lower()}_{piece}"
    return builder


# ------------------------------------------------------------------ registry

PROPS = {
    "outdoor:tree_02": build_tree_02,
    "outdoor:tree_03": build_tree_03,
    "outdoor:streetlight": build_streetlight,
    "outdoor:porch_railing_straight": build_porch_railing_straight,
    "outdoor:porch_railing_corner": build_porch_railing_corner,
    "outdoor:porch_railing_end": build_porch_railing_end,
    "outdoor:porch_post": build_porch_post,
}

for _number, _family in HOUSE_FAMILIES.items():
    for _piece in HOUSE_PIECES:
        PROPS[f"outdoor:house_{_number}_{_piece}"] = _make_house_builder(_piece, _family)
